# Railway Network

HDDS demo of a railway network simulation with trains, signals, and stations.

## Features

- **Track Network**: Oval track layout with 8 segments
- **Trains**: 3 trains with different routes and colors
- **Signals**: Traffic light signals (red/yellow/green)
- **Stations**: 4 stations with platforms
- **Real-time Visualization**: Canvas 2D with TypeScript

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p railway-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd railway-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────────────────┐      DDS       ┌──────────┐  WebSocket  ┌─────────────────┐
│      railway-sim        │ ─────────────►│  hdds-ws  │───────────►│     Browser     │
│     (simulator)         │               │  (bridge) │            │   (Canvas 2D)   │
│                         │               └──────────┘            │                 │
│  - Train physics        │                                       │  - Track view   │
│  - Signal logic         │                                       │  - Train pos    │
│  - DDS publisher        │                                       │  - Signals      │
└─────────────────────────┘                                       └─────────────────┘
```

## DDS Topics

| Topic | Type | Description |
|-------|------|-------------|
| `railway/train` | Train | Train position and state |
| `railway/signal` | Signal | Signal state changes |
| `railway/track_segment` | TrackSegment | Static track layout |
| `railway/station` | Station | Station info |
| `railway/network_state` | NetworkState | Overall statistics |

## Controls

- **Tracks**: Toggle track display
- **Trains**: Toggle train display
- **Signals**: Toggle signal display
- **Stations**: Toggle station display

## Train States

| State | Description |
|-------|-------------|
| STOPPED | Waiting at red signal |
| ACCEL | Accelerating |
| CRUISE | Constant speed |
| BRAKE | Slowing down |
| AT_STATION | Stopped at station |

## IDL Types

Types are generated from `idl/railway.idl` using hdds_gen:

```idl
module railway {
    enum SignalState { SIGNAL_RED, SIGNAL_YELLOW, SIGNAL_GREEN };
    enum TrainState { TRAIN_STOPPED, TRAIN_ACCELERATING, TRAIN_CRUISING, TRAIN_BRAKING, TRAIN_AT_STATION };

    @topic struct Train {
        @key uint32 train_id;
        string<32> name;
        uint32 color;
        uint32 current_segment;
        float position;
        float speed;
        TrainState state;
        // ...
    };
}
```

## Dependencies

- Rust + Cargo
- Node.js 18+
- HDDS (workspace dependency)
