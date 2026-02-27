# XRCE Micro Bridge Demo

HDDS demonstration showing DDS-XRCE agent bridging resource-constrained micro-clients into the full DDS domain:
- 4 simulated IoT sensor clients (temperature, humidity, pressure, light)
- XRCE agent with session management and bidirectional data flow
- Actuator command queue (agent -> micro-clients)
- Live TUI dashboard with client sessions, data feed, and throughput stats

## Quick Start

```bash
# Terminal 1: Start the sensor simulator
cargo run --release -p xrce-client-sim -- --domain-id 99 --rate-hz 1

# Terminal 2: Start the XRCE agent with TUI
cargo run --release -p xrce-agent -- --domain-id 99
```

Press `q` to quit the dashboard.

## Architecture

```
+------------------+     DDS UDP (d:99)     +------------------+
| xrce-client-sim  | --------------------> |   xrce-agent     |
| (4 IoT sensors)  |                       | (TUI dashboard)  |
+------------------+                       +------------------+
      |                                           |
      | Publishes:                                | Publishes:
      +-- MicroSensorData (1 Hz/client)          |-- XrceSessionInfo (per sample)
                                                  +-- ActuatorCommand (every 10s)
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `xrce/sensors` | MicroSensorData | 1 Hz/client | Sensor value, unit, battery %, RSSI |
| `xrce/sessions` | XrceSessionInfo | Per sample | Client session stats (streams, bytes) |
| `xrce/actuators` | ActuatorCommand | Every 10s | Commands to micro-clients (LED toggle) |

## Simulated Sensors

| Client ID | Sensor | Range | Unit | Drift Model |
|-----------|--------|-------|------|-------------|
| xrce-temp-01 | Temperature | 15-35 | C | Brownian + mean reversion |
| xrce-humid-01 | Humidity | 30-80 | %RH | Brownian + mean reversion |
| xrce-press-01 | Pressure | 990-1030 | hPa | Brownian + mean reversion |
| xrce-light-01 | Light | 0-1000 | lux | Sinusoidal day/night cycle |

All clients report battery (slow drain from 100%) and RSSI (~-65 dBm +/- 5).

## TUI Dashboard

- **Header**: Uptime, connected clients, total samples, throughput (B/s)
- **Client Sessions Table**: Client ID, transport, status (ONLINE/OFFLINE), reliable/best-effort streams, bytes in/out
- **Live Data Flow**: Last 20 readings, color-coded by sensor type (red=temp, blue=humidity, magenta=pressure, yellow=light)
- **Actuator Queue**: Recent commands with target client and value

## CLI Options

```
xrce-client-sim:
  --domain-id N       DDS domain (default: 99)
  --rate-hz N         Publishing rate per client (default: 1)
  --duration-secs N   Run duration (default: unlimited)

xrce-agent:
  --domain-id N       DDS domain (default: 99)
  --max-clients N     Max concurrent clients (default: 128)
  --refresh-ms N      TUI refresh rate (default: 100)
```

## Project Structure

```
xrce-micro-bridge/
  idl/xrce_bridge.idl       IDL definitions (3 topics)
  xrce-types/                 Generated Rust types + CDR codecs
  xrce-agent/                 XRCE agent with TUI monitor
  xrce-client-sim/            IoT sensor simulator (4 clients)
```

## Dependencies

- Rust + Cargo
- HDDS (included via workspace, with XRCE extensions)
