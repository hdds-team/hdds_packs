# UDS Fleet Diagnostic Demo

HDDS demonstration showing distributed automotive fleet diagnostics over DDS:
- 4 simulated ECUs (engine, brake, body, infotainment)
- ISO 14229 UDS protocol metrics: P2 latency, session management, firmware transfer
- Real-time health monitoring with bus load, RX/TX counters, error tracking
- AI-style diagnostic alerts (WARNING/ERROR/CRITICAL)
- Fleet-wide aggregate statistics

## Quick Start

```bash
# Terminal 1: Start the ECU bridge (publisher)
cargo run --release -p diag-bridge

# Terminal 2: Start the diagnostic monitor (subscriber)
cargo run --release -p diag-monitor
```

## Architecture

```
+--------------+     DDS UDP      +---------------+
| diag-bridge  | ---------------> | diag-monitor  |
| (4 ECUs sim) |                  | (console log) |
+--------------+                  +---------------+
      |
      | Publishes:
      |-- EcuHealth          (1 Hz, RELIABLE + TRANSIENT_LOCAL)
      |-- P2Latency          (1 Hz, BEST_EFFORT)
      |-- TransferProgress   (event, RELIABLE)
      |-- SessionChange      (event, RELIABLE)
      |-- DiagAlert          (event, RELIABLE + KEEP_ALL)
      +-- FleetStats         (1 Hz, BEST_EFFORT)
```

## Topics

| Topic | Type | QoS | Rate | Description |
|-------|------|-----|------|-------------|
| `diag/ecu/health` | EcuHealth | RELIABLE + TRANSIENT_LOCAL | 1 Hz | State, RX/TX, bus load, errors |
| `diag/ecu/latency` | P2Latency | BEST_EFFORT | 1 Hz | P2 timing: last/min/max/avg in us |
| `diag/ecu/transfer` | TransferProgress | RELIABLE | Event | Flash progress, retries, bytes |
| `diag/ecu/session` | SessionChange | RELIABLE | Event | DEFAULT <-> PROGRAMMING <-> EXTENDED |
| `diag/fleet/anomalies` | DiagAlert | RELIABLE + KEEP_ALL | Event | Diagnostic alerts with severity |
| `diag/fleet/stats` | FleetStats | BEST_EFFORT | 1 Hz | Fleet aggregates: online, flashing, errors |

## ECU Simulation

- **4 ECUs**: ecu-engine-01, ecu-brake-01, ecu-body-01, ecu-infotainment-01
- **State cycle**: IDLE -> FLASHING -> IDLE (random 1/60 chance to start flash)
- **Flash transfer**: 500-2000 blocks, 5-15 blocks/tick, with random retries
- **P2 latency**: 40-200 us with running min/max/avg
- **Bus load**: Jitter +/-5%, clamped [10%-95%]
- **Anomalies**: 1/30 chance per tick (60% WARNING, 30% ERROR, 10% CRITICAL)

## Monitor Output

```
[HEALTH] ecu-engine-01 | Flashing Programming | rx=145 tx=98 | bus=42%
[P2]     ecu-engine-01 | last=156us min=40us max=198us avg=92us
[FLASH]  ecu-engine-01 | 42% (420/1000) retries=3 | 5234ms | 107520 bytes
[ALERT !!!] ecu-engine-01 #5: Flash sector write failure detected
[FLEET]  bridge-01 | online=4 flash=1 err=0 | req=312 | bus=45%
```

## Project Structure

```
uds-fleet-diag/
  idl/huds_diag.idl          IDL definitions (3 enums, 6 topics)
  diag-types/                  Generated Rust types + enum helpers
  diag-bridge/                 ECU simulator (publisher)
  diag-monitor/                Console monitor (subscriber)
```

## Dependencies

- Rust + Cargo
- HDDS (included via workspace)
