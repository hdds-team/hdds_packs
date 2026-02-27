# NAT-WAN Topology Demo

HDDS demonstration showing NAT traversal and WAN topology visualization with:
- 3 geographically dispersed sites (Paris, Tokyo, NYC)
- ICE candidate exchange (host, server-reflexive, relay)
- NAT type simulation (full cone, symmetric, restricted)
- Real-time site heartbeats and inter-site messaging with latency
- Interactive topology viewer with connection state animation (egui)

## Quick Start

```bash
# Terminal 1: Start the topology viewer (egui)
cargo run --bin wan-topology --release

# Terminal 2-4: Start site simulators
cargo run --bin wan-site -- --site-name paris
cargo run --bin wan-site -- --site-name tokyo
cargo run --bin wan-site -- --site-name nyc
```

## Architecture

```
+-----------+     DDS UDP      +-----------+     DDS UDP      +-----------+
| wan-site  | <--------------> | wan-site  | <--------------> | wan-site  |
| (Paris)   |                  | (Tokyo)   |                  | (NYC)     |
+-----------+                  +-----------+                  +-----------+
      |                              |                              |
      +------------------------------+------------------------------+
                                     |
                                     v
                              +--------------+
                              | wan-topology |
                              | (egui viewer)|
                              +--------------+
```

## Topics

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `wan/heartbeat` | SiteHeartbeat | 1 Hz | Site health: IP, port, NAT type, RTT |
| `wan/messages` | SiteMessage | 0.5 Hz | Inter-site messages with simulated RTT |
| `wan/ice_candidates` | IceCandidate | Once | ICE candidates per site (host/srflx/relay) |

## Site Profiles

| Site | Public IP | NAT Type | Base RTT | Region |
|------|-----------|----------|----------|--------|
| Paris | 82.64.12.101 | full_cone | 15 ms | Europe |
| Tokyo | 203.104.55.22 | symmetric | 120 ms | Asia |
| NYC | 74.125.200.33 | restricted | 45 ms | N. America |

## ICE Connection Phases

```
Gathering (0-2s) --> Checking (2-4s) --> Connected (4s+)
   (gray)              (blue)            (green=direct, yellow=relay)
```

## Viewer Features

- **Topology Graph**: Sites as nodes, connections as color-coded lines
- **Connection Types**: Green (direct P2P), Yellow (TURN relay), Red (failed)
- **Heartbeat Rings**: Pulsing animation on active sites
- **Stats Panel**: Message count, avg RTT, direct vs relay ratio
- **Site Detail**: Click node for IP, NAT type, RTT, ICE candidates

## Project Structure

```
nat-wan-topology/
  idl/wan_link.idl          IDL type definitions (3 topics)
  wan-types/                 Generated Rust types + CDR codecs
  wan-site/                  Site node simulator
  wan-topology/              egui desktop topology viewer
```

## Dependencies

- Rust + Cargo
- egui + eframe (for topology viewer)
- HDDS (included via workspace)
