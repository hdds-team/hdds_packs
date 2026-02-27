# Robot Swarm

HDDS demo of coordinated robot swarm behaviors with real-time visualization.

## Features

- **12 robots** with individual colors and names
- **Swarm behaviors**:
  - Flocking (boids algorithm)
  - Formation patterns (line, circle, wedge, grid, diamond)
  - Follow leader
  - Scatter / Gather
  - Patrol waypoints
- **Obstacle avoidance**
- **Real-time visualization** with Canvas 2D
- **Status panel** with swarm metrics

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p swarm-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd swarm-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────────────────┐      DDS       ┌──────────┐  WebSocket  ┌─────────────────┐
│       swarm-sim         │ ─────────────►│  hdds-ws  │───────────►│     Browser     │
│     (simulator)         │               │  (bridge) │            │   (Canvas 2D)   │
│                         │               └──────────┘            │                 │
│  - Boids behaviors      │                                       │  - Arena view   │
│  - Formation patterns   │                                       │  - Robot markers│
│  - DDS publisher        │                                       │  - Status panel │
│                         │                                       │  - Legend       │
└─────────────────────────┘                                       └─────────────────┘
```

## DDS Topics

| Topic | Type | Description |
|-------|------|-------------|
| `swarm/robots` | Robot | Individual robot state (position, velocity, sensors) |
| `swarm/swarm_cmd` | SwarmCommand | Commands to the swarm (mode, formation, target) |
| `swarm/robot_cmd` | RobotCommand | Commands to individual robots |
| `swarm/state` | SwarmState | Aggregate swarm state (centroid, spread) |
| `swarm/obstacles` | Obstacle | Static obstacles in the arena |
| `swarm/waypoints` | Waypoint | Patrol waypoints |

## Swarm Modes

| Mode | Behavior |
|------|----------|
| Idle | Robots stay in place |
| Flock | Boids-like flocking (separation, alignment, cohesion) |
| Formation | Hold geometric pattern |
| Follow Leader | Follow the designated leader robot |
| Scatter | Spread out from centroid |
| Gather | Converge to target point |
| Patrol | Visit waypoints in sequence |

## Formation Patterns

- **Line**: Robots align horizontally
- **Circle**: Robots form a ring
- **Wedge**: V-formation with leader at front
- **Grid**: Square grid arrangement
- **Diamond**: Diamond shape

## Arena

- Size: 500cm x 500cm
- 4 static obstacles
- 5 patrol waypoints

## Dependencies

- Rust + Cargo
- Node.js 18+
- HDDS (workspace dependency)
