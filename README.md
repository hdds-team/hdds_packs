# HDDS Demos

Showcase of **[HDDS](https://github.com/hdds-team/hdds)** (High-performance DDS) capabilities through 24 self-contained demonstration projects. Each demo includes a Rust simulator/publisher, IDL type definitions, and most include a web viewer (Vite + Three.js/Canvas).

All demos use DDS domain 99.

## Demos

| Demo | Description | Viewer |
|------|-------------|--------|
| [air-defense-radar](air-defense-radar/) | Real-time radar tracking with threat classification and engagement zones | Web (Three.js) |
| [artillery-fire-control](artillery-fire-control/) | Fire mission coordination with ballistic trajectory computation | Web (Canvas) |
| [cloud-k8s](cloud-k8s/) | Kubernetes pod metrics streaming via DDS — no polling, no Prometheus | Console |
| [convoy-escort](convoy-escort/) | VIP convoy protection with real-time GPS and tactical alerts | Web (Three.js) |
| [drone-racing](drone-racing/) | 6 autonomous drones racing through gates at 60 Hz | Web (Canvas) |
| [f1tenth-racing](f1tenth-racing/) | Bicycle-physics autonomous cars with adaptive AI driving | Web (Canvas) |
| [fort-knox-security](fort-knox-security/) | DDS security dashboard — permissions, certs, audit logs in real time | Console (TUI) |
| [hdds-remote-sensor](hdds-remote-sensor/) | ESP32 + Raspberry Pi decentralized sensor over HC-12 radio | Web |
| [hot-reload-cockpit](hot-reload-cockpit/) | Live QoS policy tuning with sliders — no restarts | Console (TUI) |
| [lidar-mapping](lidar-mapping/) | Robot SLAM — autonomous maze exploration and occupancy grid mapping | Web (Canvas) |
| [metal-slug-arena](metal-slug-arena/) | 2D platformer deathmatch synchronized via DDS pub/sub | Web (Canvas) |
| [mission-recorder](mission-recorder/) | Record, replay and analyze all DDS traffic from a session | Console (TUI) |
| [nat-wan-topology](nat-wan-topology/) | DDS connectivity through NAT and firewalls with ICE | Console (TUI) |
| [naval-convoy](naval-convoy/) | Merchant ships escorted against submarines — sonar, torpedoes, depth charges | Web (Three.js) |
| [predictive-maintenance](predictive-maintenance/) | Industrial equipment degradation detection with anomaly scoring | Web (Canvas) |
| [railway-network](railway-network/) | Independent train processes coordinated by DDS signaling | Web (Canvas) |
| [robot-swarm](robot-swarm/) | 12 robots with boids, formations, and emergent behavior patterns | Web (Canvas) |
| [rpc-service-mesh](rpc-service-mesh/) | DDS-RPC service discovery, health checks, and live invocation browser | GTK4 |
| [satellite-ground-station](satellite-ground-station/) | 8 LEO satellites + 3 ground stations with orbital tracking | Web (Canvas) |
| [smart-factory](smart-factory/) | Industry 4.0 — AGVs, conveyors, and production lines without central control | Web (Canvas) |
| [space-invaders](space-invaders/) | Classic arcade game — every event is a DDS topic | Web (Canvas) |
| [tsn-automotive](tsn-automotive/) | Multi-priority ECU communication with TSN deterministic networking | Console (TUI) |
| [uds-fleet-diag](uds-fleet-diag/) | Fleet-wide ISO 14229 diagnostics — ECU health, firmware, AI alerts | Console |
| [xrce-micro-bridge](xrce-micro-bridge/) | DDS-XRCE agent bridging resource-constrained micro-clients | Console (TUI) |

## Quick Start

```bash
# Prerequisites
# - Rust toolchain (rustup.rs)
# - hddsgen (built from hdds_gen repo)
# - Node.js 18+ (for web viewers)

# 1. Generate types from IDL
make gen

# 2. Build all demos
make build

# 3. Run any demo (example: air-defense-radar)
cd air-defense-radar
cargo run --release -p radar-sim

# 4. For web viewers, also start hdds-ws + the viewer
hdds-ws --domain 99 --port 9090
cd air-defense-radar/radar-viewer && npm install && npm run dev
```

## Build Info

Each demo compiles independently. Full build of all 24 demos:

- **Source**: ~2.5 MB (284 files)
- **Build artifacts**: ~57 GB (incremental, all demos)
- **Release binaries**: ~150 MB total (31 binaries, 4-18 MB each)
- **node_modules**: ~700 MB (for web viewers, `npm install`)
- **Build time**: depends on cache state; first full build is significant

Individual demo binaries are typically 4-5 MB each (release mode).

> **Note**: Build artifacts (`target/`) are not included in the repository.
> Run `make gen` before `make build` to generate types from IDL definitions.

## Architecture Pattern

Most demos follow the same pattern:

```
demo-name/
  idl/              IDL type definitions (input for hdds_gen)
  *-types/          Generated Rust types + CDR codecs (make gen)
  *-sim/ or *-server/   Rust DDS publisher (cargo run --release)
  *-viewer/         Web viewer: Vite + JS/TS (npm run dev)
```

```
+-----------+     DDS/RTPS      +----------+    WebSocket     +------------+
|  *-sim    | ----------------> | hdds-ws  | ---------------> |  *-viewer  |
|  (Rust)   |   UDP Multicast   | (bridge) |   JSON proto     |  (Browser) |
+-----------+                   +----------+                  +------------+
```

## Makefile Targets

| Target | Description |
|--------|-------------|
| `make gen` | Regenerate all Rust types from IDL via hddsgen |
| `make build` | Build all 24 demos (reports OK/FAIL count) |
| `make gen-build` | Regenerate + build |
| `make clean` | Remove all `target/` directories |

## Dependencies

- **Rust** toolchain (stable)
- **HDDS** - included via workspace path dependencies
- **hddsgen** - IDL code generator (from hdds_gen repo)
- **hdds-ws** - WebSocket bridge (from hdds repo, `tools/hdds-ws`)
- **Node.js 18+** - for web viewers
- **GTK4** - for rpc-service-mesh fleet-explorer only
- **PlatformIO** - for hdds-remote-sensor ESP32 firmware only

## License

Apache-2.0 OR MIT
