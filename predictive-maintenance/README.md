# Predictive Maintenance Demo

HDDS demonstration showing an industrial IoT predictive maintenance dashboard with:
- Real-time sensor monitoring (temperature, vibration, pressure, current, RPM)
- Machine health scoring and status tracking
- Anomaly detection and threshold alerts
- Maintenance predictions based on degradation patterns

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p sensor-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer (dev server)
cd pm-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the DDS backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────┐      DDS UDP      ┌─────────────┐    WebSocket    ┌─────────────┐
│  sensor-sim │ ──────────────►  │   hdds-ws   │ ──────────────► │   Browser   │
│ (simulator) │                   │  (bridge)   │                  │ (Dashboard) │
└─────────────┘                   └─────────────┘                  └─────────────┘
     │                                  │
     │ Publishes:                       │ Forwards:
     ├── SensorReading (5 Hz)          ├── All topics
     ├── MachineStatus                 └── as JSON
     ├── Alert
     └── MaintenancePrediction
```

## Topics

| Topic | Description | Rate |
|-------|-------------|------|
| `pm/sensors` | Individual sensor readings | 5 Hz per sensor |
| `pm/machines` | Machine health status | 5 Hz per machine |
| `pm/alerts` | Threshold violations and anomalies | On event |
| `pm/predictions` | Maintenance predictions | Every 10s (if degrading) |

## Simulated Machines

| Machine | Sensors |
|---------|---------|
| Pump Station A | Temperature, Vibration, Pressure, Current |
| Compressor B | Temperature, Vibration, Pressure, RPM |
| Motor Drive C | Temperature, Vibration, Current, RPM |
| Heat Exchanger D | Temperature, Pressure, Flow Rate |

## Dashboard Features

- **Machines Panel**: Overview cards with health bars and status indicators
- **Sensors Panel**: Circular gauges for each sensor reading
- **Alerts Panel**: Real-time alert feed with severity colors
- **Predictions Panel**: AI-predicted failures with confidence scores

## Simulation Features

- **Normal Operation**: Sensors fluctuate around nominal values with realistic noise
- **Degradation**: Random sensors start degrading over time (0.01-0.05 per second)
- **Anomalies**: Random spike events lasting 5-30 seconds
- **Predictions**: Generated when degradation is detected, with estimated time to failure

## Dependencies

- Rust + Cargo (for simulator and bridge)
- Node.js 18+ (for viewer dev server)
- HDDS (included via workspace)
