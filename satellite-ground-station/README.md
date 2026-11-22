# Satellite Ground Station

Real-time satellite constellation tracking with orbital mechanics, ground station visibility, pass prediction, and telemetry downlink -- all published over [HDDS](https://github.com/naskel/hdds) DDS topics.

## Overview

```
sat-sim (Rust)                   hdds-ws                    sat-viewer (Browser)
  8 satellites                     DDS                        2D equirectangular
  3 ground stations          WebSocket bridge                 world map + HUD
  orbital mechanics       ws://localhost:9090/ws              orbit trails
  pass tracking                                               telemetry panels
     |                          |                                  |
     +--- UDP Multicast (DDS) --+--- JSON over WebSocket ----------+
```

**Satellites** (8): ISS, Hubble, 3x GPS-IIF, 2x Starlink, GOES-16 (LEO / MEO / GEO orbits)

**Ground Stations** (3): Svalbard SvalSat, Kourou CSG, Canberra DSN

## DDS Topics

| Topic               | Type               | QoS          | Rate              |
|----------------------|--------------------|--------------|-------------------|
| `sat/position`       | SatellitePosition  | BEST_EFFORT  | 1 Hz (8 sats)     |
| `sat/station`        | GroundStation      | BEST_EFFORT  | 1 Hz (3 stations) |
| `sat/pass`           | PassPrediction     | RELIABLE     | Event (AOS/LOS)   |
| `sat/telemetry`      | TelemetryFrame     | BEST_EFFORT  | 1 Hz (active pass) |
| `sat/alert`          | OrbitalAlert       | RELIABLE     | Event (~30s)       |

Types are defined in `idl/satellite.idl` and generated with `hddsgen`.

## Simulation

- **Orbital mechanics**: Simplified circular orbits with Keplerian velocity (`v = sqrt(GM/r)`), inclination-based latitude, Earth rotation compensation
- **Pass detection**: Edge-triggered AOS/LOS at 5 deg elevation threshold
- **Telemetry**: Battery voltage, solar power, CPU temperature, signal strength, bit errors -- only published during active passes
- **Alerts**: Conjunction warnings and orbit decay events (probabilistic, ~5% chance every 30s)
- **Signal model**: Distance-based attenuation with elevation boost and random noise

## Viewer

Canvas 2D equirectangular map projection with:

- Simplified continent outlines (6 continents)
- Satellite dots with color coding (ISS white, Hubble magenta, GPS green, Starlink cyan, GOES yellow)
- 120-point orbit trails with fading opacity
- Ground station markers with visibility circles (~2000 km radius)
- Tracking links (dashed lines from station to tracked satellite)
- Telemetry panel (top-right): live spacecraft health data
- Pass event log (bottom-left): AOS/LOS events with max elevation
- Orbital alerts (bottom-right): conjunction/decay warnings

## Quick Start

### Prerequisites

- Rust toolchain
- Node.js 18+
- [hdds-ws](https://github.com/hdds-team/hdds/tree/master/tools/hdds-ws) WebSocket bridge running on port 9090

### Run

```bash
# Terminal 1 - Simulator
cd sat-sim
cargo run --release

# Terminal 2 - Viewer
cd sat-viewer
npm install
npm run dev
# Open http://localhost:5173
```

## Project Structure

```
satellite-ground-station/
  idl/satellite.idl          IDL type definitions (3 enums, 5 structs)
  sat-types/src/
    lib.rs                   Topic constants + enum helpers
    generated.rs             hddsgen output (CDR2 encode/decode)
  sat-sim/src/main.rs        Simulator (769 lines)
  sat-viewer/
    index.html               Dark-themed HUD layout
    src/main.js              Canvas 2D renderer + WebSocket client
```

## Satellites

| Name          | NORAD | Orbit | Alt (km) | Incl   | Period   |
|---------------|-------|-------|----------|--------|----------|
| ISS           | 25544 | LEO   | 408      | 51.6   | 92.5 min |
| Hubble        | 20580 | LEO   | 547      | 28.5   | 95.5 min |
| GPS-IIR-14    | 28874 | MEO   | 20,200   | 55.0   | 718 min  |
| GPS-IIF-09    | 40294 | MEO   | 20,200   | 55.0   | 718 min  |
| GPS-IIF-12    | 41328 | MEO   | 20,200   | 55.0   | 718 min  |
| Starlink-1007 | 44713 | LEO   | 550      | 53.0   | 95 min   |
| Starlink-1008 | 44714 | LEO   | 550      | 53.0   | 95 min   |
| GOES-16       | 36744 | GEO   | 35,786   | 0.0    | 1436 min |

## Ground Stations

| ID  | Name           | Location          | Lat/Lon          |
|-----|----------------|-------------------|------------------|
| SVB | Svalbard SvalSat | Norway (Arctic)  | 78.2N, 15.6E    |
| KOU | Kourou CSG     | French Guiana      | 5.2N, 52.8W     |
| CAN | Canberra DSN   | Australia          | 35.3S, 149.0E   |
