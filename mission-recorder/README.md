# Mission Recorder Demo

HDDS demonstration showing a complete simulate-record-replay pipeline for naval patrol operations:
- 4-unit maritime patrol simulation (Alpha, Bravo, Charlie, Delta)
- Position updates, contact detections, and communications
- Recording to InfluxDB Line Protocol format
- Interactive TUI replay viewer with timeline and ASCII map

## Quick Start

```bash
# Terminal 1: Start the patrol simulator
cargo run --bin recorder-sim

# Terminal 2: Start the recorder (writes .lp file)
cargo run --bin recorder-sink

# After simulation completes (~5 min), replay:
cargo run --bin recorder-replay
# or with a specific file:
cargo run --bin recorder-replay -- mission_recording.lp
```

Replay controls: `Space` play/pause, `Left/Right` seek 10s, `1-4` speed (1x/2x/4x/8x), `q` quit.

## Architecture

```
+--------------+     DDS UDP      +---------------+     File      +----------------+
| recorder-sim | ---------------> | recorder-sink | -----------> | recording.lp   |
| (4 units)    |                  | (LP writer)   |              | (InfluxDB LP)  |
+--------------+                  +---------------+              +----------------+
                                                                        |
                                                                        v
                                                                +----------------+
                                                                | recorder-replay|
                                                                | (TUI viewer)   |
                                                                +----------------+
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `mission/event` | MissionEvent | 2 Hz/unit | Position, contact, comm events with payload |
| `mission/status` | MissionStatus | 0.5 Hz | Mission phase, elapsed time, event count |

## Mission Phases

| Phase | Time | Description |
|-------|------|-------------|
| Planning | 0-30s | Initial setup |
| Transit | 30-120s | Units moving to patrol area |
| Engagement | 120-240s | Active patrol, contacts possible (5%/tick) |
| RTB | 240-300s | Return to base |

## Event Types

- **position**: Unit lat/lon/alt/heading/speed (2 Hz per unit)
- **contact**: Bearing, range, classification (5% chance during engagement)
- **comm**: Radio messages on specific frequencies (2% chance any phase)

## TUI Replay Viewer

- **Event Log**: Color-coded entries (red=contact, yellow=comm, magenta=status)
- **ASCII Map**: Auto-scaled with unit markers (A/B/C/D)
- **Timeline**: Progress bar with current/total time
- **Stats**: Total events, duration, playback speed

## Project Structure

```
mission-recorder/
  idl/mission.idl           IDL type definitions (2 topics)
  recorder-types/            Generated Rust types + CDR codecs
  recorder-sim/              4-unit patrol simulator
  recorder-sink/             DDS to InfluxDB Line Protocol writer
  recorder-replay/           Interactive TUI replay viewer (ratatui)
  mission_recording.lp       Pre-recorded sample (~845 events)
```

## Dependencies

- Rust + Cargo
- HDDS (included via workspace)
- hdds-influx-sink (InfluxDB LP writer)
