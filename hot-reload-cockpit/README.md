# Hot-Reload Cockpit Demo

HDDS demonstration showing runtime QoS (Quality of Service) hot-reload with:
- Aircraft cockpit sensor simulation (airspeed, altitude, heading, temperature)
- Live QoS parameter adjustment via desktop GUI sliders
- YAML-based QoS profiles (default, combat, eco) with file watching
- Before/after metrics showing impact of QoS changes
- Timestamped audit log of all QoS modifications

## Quick Start

```bash
# Terminal 1: Start the sensor simulator
cargo run --bin cockpit-sim --release

# Terminal 2: Start the control panel (FLTK GUI)
cargo run --bin cockpit-panel --release
```

## Architecture

```
+---------------+     DDS UDP      +-----------------+
|  cockpit-sim  | <--------------> |  cockpit-panel  |
| (sensor pub)  |                  | (FLTK GUI)      |
+---------------+                  +-----------------+
      |                                   |
      | Publishes:                        | Controls:
      |-- SensorFeed (10 Hz)             |-- QoS sliders (deadline, latency, priority)
      |-- QosSnapshot (1 Hz)             |-- Profile selector (default/combat/eco)
      +-- MetricsDelta (on change)       +-- Writes YAML -> file watcher detects
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `cockpit/sensor` | SensorFeed | 10 Hz | Sensor value + quality (0-1) |
| `cockpit/qos_snapshot` | QosSnapshot | 1 Hz | Current QoS policy state |
| `cockpit/metrics_delta` | MetricsDelta | On change | Before/after latency, deadlines, throughput |

## QoS Profiles

| Profile | Deadline | Latency Budget | Priority | Use Case |
|---------|----------|---------------|----------|----------|
| default | 100 ms | 10 ms | 3 | Normal operations |
| combat | 10 ms | 1 ms | 7 | Low-latency, high-priority |
| eco | 500 ms | 50 ms | 1 | Bandwidth-saving |

## Panel Features

- **Profile Selector**: Dropdown to switch QoS profiles
- **Live Sensors**: Real-time airspeed, altitude, heading, temperature with quality %
- **QoS Sliders**: Deadline (1-1000 ms), Latency Budget (0-100 ms), Priority (0-10)
- **Apply Button**: Writes YAML, simulator detects change within 2s
- **Metrics Display**: Latency, throughput, missed deadlines (before/after)
- **Change Log**: Scrollable timestamped history of QoS modifications

## Hot-Reload Mechanism

1. Panel writes QoS changes to YAML profile file
2. Simulator's `QosFileWatcher` polls every 2 seconds
3. On change: mutable QoS policies updated on live DDS entities
4. `MetricsDelta` published showing improvement (latency -20%, deadlines reset)

## Project Structure

```
hot-reload-cockpit/
  idl/cockpit.idl          IDL type definitions (3 topics)
  cockpit-types/            Generated Rust types + CDR codecs
  cockpit-sim/              Sensor simulator with QoS file watcher
  cockpit-panel/            FLTK desktop GUI
  qos-profiles/             YAML profiles (default.yaml, combat.yaml)
```

## Dependencies

- Rust + Cargo
- FLTK (for cockpit-panel native GUI)
- HDDS (included via workspace)
