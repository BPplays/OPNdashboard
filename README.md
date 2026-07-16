# OPNsense Gateway Dashboard

A Rust-based dashboard that aggregates gateways from OPNsense router and displays user-friendly status information.

## Features

- Reads configuration from YAML file specifying which gateways to monitor
- Collects data from OPNsense API endpoint `/api/routing/settings/search_gateway/`
- Aggregates gateway status using median (for 3+ gateways) or mean (fewer than 3) calculations for latency and loss
- Provides a RESTful API with aggregated gateway information
- Configurable server port (via config.yaml or PORT environment variable)
- Default listen address set to [::] if not specified

## Configuration

Create a `config.yaml` file in the project root:

```yaml
gateways:
  - name: "WAN"
    gateway_names:
      - "WAN_DHCP6"
      - "WAN_GW"

opnsense:
  url: "https://your-opnsense-host"
server:
  listen: "[::]"
  port: 3000
```

## Running with Custom Port

You can override the port specified in config.yaml using the PORT environment variable:

```bash
PORT=8080 cargo run
```

## API Endpoints

- `GET /health` - Health check endpoint
- `GET /gateways` - Get aggregated gateway status information

## Building and Running

```bash
# Build the project
cargo build

# Run the server with default port (3000)
cargo run

# Run in release mode
cargo run --release

# Run with custom port via environment variable
PORT=8080 cargo run
```

The server will start on the configured port.

## Implementation Details

1. **Configuration Loading**: Parses YAML configuration file to understand which gateway groups to monitor
2. **API Integration**: Makes requests to OPNsense API to fetch gateway status data
3. **Data Aggregation**: 
   - Calculates median for latency and loss when 3+ gateways are present
   - Calculates mean when fewer than 3 gateways are present
   - Uses status aggregation with offline > online > unknown priority
4. **REST API**: Exposes aggregated information via simple JSON endpoints

## Example Response

```json
{
  "WAN": {
    "name": "WAN",
    "status": "Offline",
    "latency": "0.0 ms",
    "loss": "100.0 %",
    "gateways": [
      {
        "name": "WAN_DHCP6",
        "status": "Offline",
        "latency": "0.0 ms",
        "loss": "100.0 %"
      },
      {
        "name": "WAN_GW",
        "status": "Offline",
        "latency": "0.0 ms",
        "loss": "100.0 %"
      }
    ]
  }
}
```