# Air Defense Radar Demo

HDDS demonstration showing a military-style air defense radar system with:
- Rotating radar sweep with phosphor trail effect
- Aircraft tracks (friendly, hostile, neutral)
- SAM sites with engagement logic
- Missile intercepts and explosions

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p radar-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer (dev server)
cd radar-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the DDS backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────┐      DDS UDP      ┌─────────────┐    WebSocket    ┌─────────────┐
│  radar-sim  │ ──────────────►  │   hdds-ws   │ ──────────────► │   Browser   │
│ (simulator) │                   │  (bridge)   │                  │  (Canvas)   │
└─────────────┘                   └─────────────┘                  └─────────────┘
     │                                  │
     │ Publishes:                       │ Forwards:
     ├── RadarSweep (90°/s)            ├── All topics
     ├── AircraftTrack                 └── as JSON
     ├── SamSite
     ├── Engagement
     ├── EngagementResult
     └── RadarAlert
```

## Topics

| Topic | Description | Rate |
|-------|-------------|------|
| `radar/sweep` | Radar beam position (0-360°) | 10 Hz |
| `radar/tracks` | Aircraft positions and status | 10 Hz per track |
| `radar/sam_sites` | SAM site status and missiles | On change |
| `radar/engagements` | Active missile engagements | 10 Hz |
| `radar/engagement_results` | Kill/miss results | On event |
| `radar/alerts` | Threat warnings | On event |

## Viewer Features

- **Radar Scope**: Canvas 2D with phosphor-style sweep trail
- **Range Rings**: 50km intervals up to 200km
- **Track Symbols**: Color-coded by status (red=hostile, blue=friendly, yellow=neutral)
- **SAM Coverage**: Yellow circles showing engagement range
- **Engagements**: Red dashed lines from SAM to target, animated missile
- **Explosions**: Flash effect on successful intercepts
- **Stats Panel**: Track counts, engagements, sweep angle
- **Alerts Panel**: Real-time threat notifications

## Configuration

The demo uses UDP multicast for DDS discovery by default.
No configuration needed for local testing.

## Dependencies

- Rust + Cargo (for simulator and bridge)
- Node.js 18+ (for viewer dev server)
- HDDS (included via workspace)
