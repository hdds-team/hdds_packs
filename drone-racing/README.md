# Drone Racing 3D

HDDS 3D drone racing demo with Three.js visualization!

## Features

- **3D Graphics**: Three.js with neon-style drones and gates
- **Track**: 10 gates in a circuit with varying heights
- **Physics**: Realistic drone flight (thrust, drag, gravity)
- **AI Racers**: 6 AI drones competing
- **HUD**: Speed, position, lap counter, leaderboard
- **Minimap**: Top-down view of track and drone positions

## Quick Start

```bash
# Terminal 1: Start the race server
cargo run -p racing-server

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer (dev server)
cd racing-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the race server, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────────┐     DDS UDP      ┌─────────────────┐
│  racing-server  │ ──────────────►  │     Browser     │
│  (race logic)   │                  │  (Three.js 3D)  │
│                 │    WebSocket     │                 │
│  - Physics      │ ────────────────►│  - 3D Scene     │
│  - Gate detect  │                  │  - Camera       │
│  - AI pilots    │                  │  - HUD          │
│  - Leaderboard  │                  │  - Minimap      │
└─────────────────┘                  └─────────────────┘
```

## Topics

| Topic | Description | Direction |
|-------|-------------|-----------|
| `race/drones` | Drone positions & state | Server → Client |
| `race/inputs` | Player inputs | Client → Server |
| `race/gates` | Gate positions | Server → Client |
| `race/gate_pass` | Gate pass events | Server → Client |
| `race/state` | Race state (countdown, etc.) | Server → Client |
| `race/leaderboard` | Leaderboard updates | Server → Client |

## Controls (Future)

| Key | Action |
|-----|--------|
| W / S | Pitch (up/down) |
| A / D | Yaw (turn left/right) |
| Q / E | Roll (bank left/right) |
| Shift | Throttle |

## Track

10 gates forming a circuit:
- Start/Finish line (green gate)
- 9 checkpoints (cyan gates)
- Varying heights (10m to 35m)
- 3 laps to complete

## AI Drones

| Name | Color |
|------|-------|
| PHOENIX | Pink |
| VIPER | Green |
| STORM | Purple |
| BLAZE | Yellow |
| NOVA | Cyan |
| SHADOW | Orange |

## Dependencies

- Rust + Cargo (for race server)
- Node.js 18+ (for viewer dev server)
- Three.js (included via npm)
- HDDS (included via workspace)
