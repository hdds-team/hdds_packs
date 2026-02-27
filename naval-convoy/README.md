# Naval Convoy

HDDS demonstration showing a naval convoy being hunted by enemy submarines.

## Features

- **Convoy simulation**: Merchant ships protected by destroyer/frigate escorts
- **Submarine warfare**: Enemy subs stalking the convoy
- **Sonar detection**: Active sonar pings with expanding radius visualization
- **Combat**: Torpedoes, depth charges, and explosions
- **3D visualization**: Three.js with custom ocean shader
- **Real-time minimap**: Top-down tactical view
- **Combat log**: Event feed with priority highlighting

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p naval-sim --release

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd naval-viewer
npm install
npm run dev
```

Open http://localhost:5173 in your browser.

## Architecture

```
┌──────────────────┐         WebSocket         ┌──────────────────┐
│                  │ ────────────────────────► │                  │
│   naval-sim      │       JSON messages       │   naval-viewer   │
│   (Rust)         │                           │   (Three.js)     │
│                  │                           │                  │
│  - Convoy AI     │                           │  - 3D Ocean      │
│  - Submarine AI  │                           │  - Ship models   │
│  - Combat logic  │                           │  - Sonar effects │
│  - Sonar system  │                           │  - Explosions    │
└──────────────────┘                           └──────────────────┘
```

## Controls

| Key/Button | Action |
|------------|--------|
| Top View | Bird's eye view centered on origin |
| Follow Convoy | Camera follows convoy center |
| Free Camera | Manual camera control |
| W/A/S/D | Move camera (free mode) |
| Q/E | Move camera up/down (free mode) |

## Topics (DDS/IDL)

See `idl/naval.idl` for complete type definitions.

| Topic | Description | Rate |
|-------|-------------|------|
| Ship | Surface vessel state | 30 Hz |
| Submarine | Submarine state | 30 Hz |
| SonarPing | Active sonar emissions | ~0.25 Hz |
| SonarContact | Detected contacts | On detection |
| Torpedo | Torpedo state | 30 Hz |
| DepthCharge | Depth charge state | 30 Hz |
| Explosion | Combat explosions | On event |
| CombatEvent | Combat log entries | On event |
| ConvoyStatus | Convoy summary | 30 Hz |
| SeaState | Environmental conditions | 1 Hz |

## Scenario

**Friendly Forces (CONVOY ALPHA)**
- 3 merchant ships (cargo, tanker)
- 4 escort warships (2 destroyers, 2 frigates)
- Active sonar, depth charges

**Hostile Forces**
- 3 attack submarines (K-219, K-322, K-141 Kursk)
- Torpedoes
- Stealth approach tactics

## Customization

Edit `naval-sim/src/main.rs` to modify:
- Convoy composition and formation
- Submarine tactics and aggression
- Sonar detection parameters
- Weapon characteristics

## Dependencies

**Simulator (Rust)**
- tokio (async runtime)
- tokio-tungstenite (WebSocket)
- serde/serde_json (serialization)
- rand (RNG)

**Viewer (JavaScript)**
- Three.js (3D graphics)
- Vite (build tool)

## License

MIT OR Apache-2.0
