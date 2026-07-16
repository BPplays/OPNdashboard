use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Html},
    routing::get,
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Parser, Debug)]
#[command(name = "opn-dashboard")]
struct Args {
    /// Path to configuration file
    #[arg(short, long, default_value = "config.yaml")]
    config: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct GatewayConfig {
    name: String,
    gateway_names: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct OpnsenseConfig {
    url: String,
    api_key: String,
    api_secret: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct ServerConfig {
    listen: Option<String>,
    port: u16,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Config {
    gateways: Vec<GatewayConfig>,
    opnsense: OpnsenseConfig,
    server: Option<ServerConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
struct GatewayResponse {
    disabled: bool,
    name: String,
    #[serde(rename = "ipprotocol")]
    ip_protocol: String,
    gateway: String,
    defaultgw: bool,
    monitor: String,
    #[serde(rename = "status")]
    status_str: String,
    delay: String,
    stddev: String,
    loss: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct GatewayApiResult {
    rows: Vec<GatewayResponse>,
}

#[derive(Debug, Clone)]
struct AppState {
    config: Config,
    client: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct AggregatedGateway {
    name: String,
    status: String,
    latency: Option<String>,
    loss: Option<String>,
    gateways: Vec<GatewayStatus>,
}

#[derive(Debug, Serialize)]
struct GatewayStatus {
    name: String,
    status: String,
    latency: Option<String>,
    loss: Option<String>,
}

impl From<GatewayResponse> for GatewayStatus {
    fn from(response: GatewayResponse) -> Self {
        let delay = if response.delay == "~" {
            None
        } else {
            Some(response.delay)
        };

        let loss = if response.loss == "~" {
            None
        } else {
            Some(response.loss)
        };

        GatewayStatus {
            name: response.name,
            status: response.status_str,
            latency: delay,
            loss,
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    println!("start with config: {}", args.config.to_string());

    let config_content = std::fs::read_to_string(&args.config)
        .expect("Failed to read config file");
    let config: Config = serde_yaml::from_str(&config_content)
        .expect("Failed to parse YAML config");

    let client = reqwest::Client::new();
    let app_state = AppState { config, client };

    let server_config = &app_state.config.server;
    let (listen_addr, port) = if let Some(server_config) = server_config {
        (server_config.listen.clone().unwrap_or_else(|| "[::1]".to_string()), server_config.port)
    } else {
        ("[::1]".to_string(), 3000)
    };

    let app = Router::new()
        .route("/api/gateways", get(get_gateways))
        .route("/gateways", get(gateways_page))
        .route("/page-gateways", get(gateways_page))
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind(format!("{}:{}", listen_addr, port))
        .await
        .unwrap();

    println!("Server running on http://{}:{}", listen_addr, port);

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn health_check() -> impl IntoResponse {
    StatusCode::OK
}

async fn get_gateways(State(state): State<AppState>) -> impl IntoResponse {
    let mut response = HashMap::<String, AggregatedGateway>::new();

    for gateway_config in &state.config.gateways {
        let gateways = fetch_gateways(&state.client, &state.config.opnsense.url, &gateway_config.gateway_names).await;
        let agated_gateway = aggregate_gateway_data(gateway_config.name.clone(), gateways);
        response.insert(gateway_config.name.clone(), agated_gateway);
    }

    Json(response)
}

async fn gateways_page() -> impl IntoResponse {
    Html(include_str!("../web/gateways.html").to_string())
}

async fn fetch_gateways(
    client: &reqwest::Client,
    opn_url: &str,
    gateway_names: &[String],
) -> Vec<GatewayResponse> {
    let full_url = format!("{}/api/routing/settings/search_gateway/", opn_url);

    let response = client
        .get(&full_url)
        .send()
        .await
        .expect("Failed to send request");

    let result: GatewayApiResult = response.json().await.expect("Failed to parse JSON");

    result.rows.into_iter()
        .filter(|row| gateway_names.contains(&row.name))
        .collect()
}

fn aggregate_gateway_data(name: String, gateways: Vec<GatewayResponse>) -> AggregatedGateway {
    let gateway_statuses: Vec<GatewayStatus> = gateways.into_iter().map(|g| g.into()).collect();

    let status = if gateway_statuses.iter().any(|g| g.status == "Offline") {
        "Offline".to_string()
    } else if gateway_statuses.iter().any(|g| g.status == "Online") {
        "Online".to_string()
    } else {
        "Unknown".to_string()
    };

    let (avg_delay, avg_loss) = calculate_aggregates(&gateway_statuses);

    AggregatedGateway {
        name,
        status,
        latency: avg_delay,
        loss: avg_loss,
        gateways: gateway_statuses,
    }
}

fn calculate_aggregates(gateways: &[GatewayStatus]) -> (Option<String>, Option<String>) {
    if gateways.is_empty() {
        return (None, None);
    }

    let use_median = gateways.len() >= 3;

    let delays: Vec<Option<f64>> = gateways
        .iter()
        .filter_map(|g| {
            if let Some(delay_str) = &g.latency {
                if delay_str != "~" {
                    return Some(delay_str.replace(" ms", "").parse::<f64>().ok());
                }
            }
            None
        })
        .collect();

    let losses: Vec<Option<f64>> = gateways
        .iter()
        .filter_map(|g| {
            if let Some(loss_str) = &g.loss {
                if loss_str != "~" {
                    return Some(loss_str.replace(" %", "").parse::<f64>().ok());
                }
            }
            None
        })
        .collect();

    let avg_delay = if !delays.is_empty() {
        let value = if use_median && delays.len() >= 3 {
            calculate_median(&delays)
        } else {
            calculate_mean(&delays)
        };
        Some(format!("{:.1} ms", value))
    } else {
        None
    };

    let avg_loss = if !losses.is_empty() {
        let value = if use_median && losses.len() >= 3 {
            calculate_median(&losses)
        } else {
            calculate_mean(&losses)
        };
        Some(format!("{:.1} %", value))
    } else {
        None
    };

    (avg_delay, avg_loss)
}

fn calculate_mean(values: &[Option<f64>]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let sum: f64 = values.iter().filter_map(|v| *v).sum();
    sum / values.len() as f64
}

fn calculate_median(values: &[Option<f64>]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut sorted_values = values.iter().filter_map(|v| *v).collect::<Vec<f64>>();
    sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let len = sorted_values.len();
    if len % 2 == 0 {
        (sorted_values[len / 2 - 1] + sorted_values[len / 2]) / 2.0
    } else {
        sorted_values[len / 2]
    }
}
