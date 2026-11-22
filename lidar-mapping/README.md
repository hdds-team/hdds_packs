# LiDAR Mapping

HDDS demo of SLAM (Simultaneous Localization and Mapping) with simulated LiDAR sensor.

## Features

- **Simulated LiDAR**: 360° scanning with raycasting
- **Occupancy grid mapping**: Real-time map building
- **Maze environment**: Multiple rooms and corridors
- **Autonomous exploration**: Simple obstacle avoidance
- **Interactive viewer**: Pan, zoom, toggle layers

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p lidar-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd lidar-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────────────────┐     WebSocket      ┌─────────────────┐
│       lidar-sim         │ ──────────────────►│     Browser     │
│   (simulator + SLAM)    │                    │   (Canvas 2D)   │
│                         │                    │                 │
│  - Raycasting LiDAR     │                    │  - Occupancy map│
│  - Occupancy grid       │                    │  - LiDAR rays   │
│  - Exploration AI       │                    │  - Robot pose   │
│  - DDS publisher        │                    │  - Controls     │
└─────────────────────────┘                    └─────────────────┘
```

## DDS Topics

| Topic | Type | Description |
|-------|------|-------------|
| `RobotPose` | RobotPose | Robot position and velocity |
| `LidarBatch` | LidarBatch | Batched LiDAR readings |
| `MapBatch` | MapBatch | Occupancy grid updates |
| `WallSegment` | WallSegment | Environment walls |

**The web viewer cannot show the scans yet.** `LidarBatch` and `MapBatch` each
carry a `sequence<>` member, and the `hdds-ws` bridge delivers those samples to
the browser as opaque bytes: measured 584 of 584 and 1 of 1 over one run, while
`RobotPose`, which has no sequence, decodes every time. So the robot moves on
screen and the map stays empty. A native DDS reader using the generated type
decodes them normally; this is a limit of the bridge's dynamic decoding.

## Controls

- **Mouse drag**: Pan view (when not following robot)
- **Mouse wheel**: Zoom in/out
- **Walls**: Toggle ground truth wall display
- **LiDAR**: Toggle LiDAR beam display
- **Map**: Toggle occupancy grid display
- **Follow**: Toggle robot following

## Environment

- Size: 1000cm x 1000cm
- Maze-like structure with rooms and corridors
- Static obstacles
- LiDAR range: 300cm
- Map resolution: 5cm per cell

## Visualization

- **Green cells**: Free space (confirmed empty)
- **Red cells**: Occupied (obstacles detected)
- **Blue lines**: Walls (ground truth)
- **Red rays**: LiDAR beams
- **Orange circle**: Robot with heading indicator

## Dependencies

- Rust + Cargo
- Node.js 18+
- HDDS (workspace dependency)
