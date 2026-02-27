# Fort Knox Security Demo

HDDS demonstration showing a bank vault security system with DDS Security concepts:
- 20 multi-type sensors (camera, motion, door, vibration, thermal, weight) across 5 zones
- Real-time breach detection and tamper alerts
- Zone-based access control with clearance levels
- Vault monitoring (door state, weight anomaly, environment)
- Audit trail with sequence-numbered entries
- Dual GUI: GTK4 vault monitor + egui admin panel

## Quick Start

```bash
# Terminal 1: Start the sensor simulator
cargo run --release -p knox-sensors

# Terminal 2: Start the vault monitor (GTK4)
cargo run --release -p knox-monitor

# Terminal 3: Start the admin panel (egui)
cargo run --release -p knox-admin
```

## Architecture

```
+--------------+     DDS UDP      +---------------+
| knox-sensors | ---------------> | knox-monitor  |
| (20 sensors) |                  | (GTK4 vault)  |
+--------------+                  +---------------+
      |
      |  DDS UDP      +---------------+
      +-------------> |  knox-admin   |
                      | (egui panel)  |
                      +---------------+
                            |
                            | Publishes:
                            +-- PermissionChange (on-demand)

Sensors publish:
  |-- SensorReading    (2 Hz, 20 sensors)
  |-- SecurityAlert    (5-15s intervals)
  |-- AccessEvent      (3-8s intervals)
  |-- VaultStatus      (every 2s)
  +-- AuditLogEntry    (variable)
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `knox/sensors` | SensorReading | 2 Hz | Raw sensor data with confidence and tamper flag |
| `knox/alerts` | SecurityAlert | 5-15s | INFO/WARNING/CRITICAL/BREACH alerts |
| `knox/access` | AccessEvent | 3-8s | Personnel zone transitions (granted/denied) |
| `knox/vault` | VaultStatus | 2s | Door state, temp, humidity, weight, anomaly |
| `knox/audit` | AuditLogEntry | Variable | Immutable audit trail (tampers, alerts, access) |
| `knox/permissions` | PermissionChange | On-demand | Admin-issued permission grants/revokes |

## Security Zones

| Zone | Name | Access Level |
|------|------|-------------|
| 0 | PUBLIC_LOBBY | All |
| 1 | TELLER_AREA | Staff |
| 2 | VAULT_CORRIDOR | Managers |
| 3 | VAULT_ROOM | Security |
| 4 | SERVER_ROOM | IT/Admin only |

## Viewer Features

**GTK4 Vault Monitor:**
- Zone status badges (color-coded: green/orange/red)
- Sensor readings grid with confidence %
- Vault status panel (door seal, weight, environment)
- Access events log (granted green / denied red)
- Security alerts feed (severity-colored)

**egui Admin Panel (5 tabs):**
- Permissions: per-subject zone toggle grid
- Audit Log: full-text searchable event history
- Certificates: DDS Security PKI (X.509 + DH)
- Dynamic Rules: time-based / condition-based access rules
- Metrics: alert severity breakdown, access stats, DDS Security config

## Project Structure

```
fort-knox-security/
  idl/fort_knox.idl       IDL type definitions (3 enums, 6 topics)
  knox-types/              Generated Rust types + CDR codecs
  knox-sensors/            Sensor simulator (async, publishes 5 topics)
  knox-monitor/            GTK4 vault dashboard
  knox-admin/              egui admin panel
```

## Dependencies

- Rust + Cargo
- GTK4 (for knox-monitor)
- HDDS (included via workspace)
