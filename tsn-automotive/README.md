# TSN Automotive Demo

HDDS demonstration showing Time-Sensitive Networking for automotive ECU communication:
- Multi-tier priority system: CRITICAL (brake/steering), HIGH (telemetry), LOW (metrics)
- Hardware TSN probing (ETF, TAPRIO, MQPRIO qdiscs) with graceful fallback
- QoS-to-TSN mapping (PCP 802.1p, DSCP code points)
- Realistic vehicle dynamics simulation (speed, RPM, braking, steering)
- TUI dashboard with latency waterfall, vehicle gauges, and deadline tracking

## Quick Start

```bash
# Terminal 1: Start the ECU simulator
cargo run --release -p tsn-sim

# Terminal 2: Start the TUI dashboard
cargo run --release -p tsn-dashboard
```

Press `q` to quit the dashboard.

## Architecture

```
+-----------+     DDS UDP      +----------------+
|  tsn-sim  | ---------------> | tsn-dashboard  |
| (ECU sim) |                  | (ratatui TUI)  |
+-----------+                  +----------------+
      |
      | Publishes:
      |-- BrakeCommand       (100 Hz, CRITICAL, PCP 7)
      |-- SteeringCommand    (100 Hz, CRITICAL, PCP 7)
      |-- VehicleTelemetry   (20 Hz,  HIGH,     PCP 4)
      +-- TsnMetrics         (1 Hz,   LOW,      PCP 0)
```

## Topics

| Topic | Type | Priority | Rate | Deadline | DSCP |
|-------|------|----------|------|----------|------|
| `tsn/brake` | BrakeCommand | CRITICAL | 100 Hz | 1 ms | 46 (EF) |
| `tsn/steering` | SteeringCommand | CRITICAL | 100 Hz | 1 ms | 46 (EF) |
| `tsn/telemetry` | VehicleTelemetry | HIGH | 20 Hz | 10 ms | 34 (AF41) |
| `tsn/metrics` | TsnMetrics | LOW | 1 Hz | None | 0 (BE) |

## TSN Priority Mapping

| Tier | Transport Priority | PCP (802.1p) | DSCP | QoS |
|------|--------------------|-------------|------|-----|
| CRITICAL | 100 | 7 | 46 (EF) | RELIABLE, 1ms deadline |
| HIGH | 50 | 4 | 34 (AF41) | RELIABLE, 10ms deadline |
| LOW | 0 | 0 | 0 (BE) | BEST_EFFORT |

## Vehicle Simulation

- **Speed**: Sinusoidal 40-130 km/h with jitter
- **RPM**: Derived from speed via 4-speed gear ratios
- **Braking**: Cyclical activation, 0-120 bar, ABS triggers >60 bar (30% probability)
- **Emergency**: 0.1% random trigger, sets max brake + ABS
- **Steering**: Dual-frequency sine (0.3 Hz + 0.7 Hz), range [-45, +45] degrees
- **Fuel**: Continuous drain (-1%/100s)
- **Battery**: 13.8V +/- 0.3V sinusoidal
- **Oil Temp**: Follows RPM with thermal lag, range [60, 130] C

## TUI Dashboard

- **Latency Waterfall**: p99 latency per tier with deadline health (green/yellow/red)
- **Vehicle Gauges**: Speed, RPM, fuel, battery, oil temp with ASCII progress bars
- **TSN Metrics Table**: Packets sent, on-time %, late count, p99, jitter per tier
- **Event Log**: Last 50 brake/steer events, emergency highlighted in red

## Project Structure

```
tsn-automotive/
  idl/tsn_vehicle.idl       IDL definitions (1 enum, 4 topics)
  tsn-types/                  Generated Rust types + CDR codecs
  tsn-sim/                    ECU simulator with TSN hardware probing
  tsn-dashboard/              ratatui TUI with latency monitoring
```

## Dependencies

- Rust + Cargo
- HDDS (included via workspace, with TSN extensions)
- Linux kernel with TSN support (optional, graceful fallback)
