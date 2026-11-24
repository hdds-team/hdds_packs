# Convoy Escort Demo

Real-time VIP convoy tracking demo showcasing **HDDS** (High-performance DDS) with Three.js visualization.

## Architecture

```
┌─────────────┐      DDS/RTPS       ┌───────────┐     WebSocket      ┌───────────────┐
│ convoy-sim  │ ◄─────────────────► │  hdds-ws  │ ◄────────────────► │ convoy-viewer │
│   (Rust)    │   UDP Multicast     │  (bridge) │   JSON protocol    │   (Browser)   │
│             │                     │           │                    │   Three.js    │
│ Publishes:  │                     │ Converts: │                    │               │
│ - GPS 10Hz  │                     │ CDR2↔JSON │                    │ Displays:     │
│ - State 1Hz │                     │           │                    │ - 3D Terrain  │
│ - Alerts    │                     │           │                    │ - Vehicles    │
└─────────────┘                     └───────────┘                    │ - Minimap     │
                                                                     └───────────────┘
```

## Features

- **Real DDS/RTPS**: Uses HDDS with UDP multicast discovery
- **IDL-generated types**: convoy.idl → Rust + TypeScript via hdds_gen
- **WebSocket bridge**: hdds-ws provides browser connectivity
- **3D visualization**: Three.js with procedural terrain
- **Real-time tracking**: GPS positions at 10Hz
- **Tactical minimap**: Top-down view of convoy

## Quick Start

### 1. Build the simulator

```bash
cd convoy-escort
cargo build --release
```

### 2. Start hdds-ws (WebSocket bridge)

```bash
# From hdds repo
hdds-ws --domain 99 --port 9090
```

### 3. Start the convoy simulator

```bash
./target/release/convoy-sim
```

### 4. Start the viewer

```bash
cd convoy-viewer
npm install
npm run dev
```

Open http://localhost:5200 in your browser.

## Topics (DDS)

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `convoy/vehicle/gps` | VehicleGPS | 10 Hz | GPS position, heading, speed |
| `convoy/vehicle/state` | VehicleState | 1 Hz | Fuel, status, passengers |
| `convoy/summary` | ConvoySummary | 1 Hz | Convoy aggregate status |
| `convoy/alert` | ConvoyAlert | On event | Threats, roadblocks |

## IDL Types

See `idl/convoy.idl` for complete type definitions.

Generated code:
- **Rust**: `convoy-types/src/lib.rs`
- **TypeScript**: `convoy-viewer/src/types.ts`

Regenerate with:
```bash
idl-gen gen rust idl/convoy.idl -o convoy-types/src/lib.rs
idl-gen gen typescript idl/convoy.idl -o convoy-viewer/src/types.ts
```

## Scenario

**VIP Escort Paris → Berlin**

Convoy composition:
- LEAD-1: Lead escort vehicle (Humvee)
- VIP-EAGLE: Protected VIP vehicle
- ESCORT-2: Side escort
- REAR-3: Rear guard

The convoy travels east at ~60 km/h. Random alerts (roadblocks, hazards) appear periodically.

## Project Structure

```
convoy-escort/
├── Cargo.toml              # Rust workspace
├── idl/
│   └── convoy.idl          # IDL type definitions
├── convoy-types/           # Generated Rust types
│   └── src/lib.rs          # hdds_gen output
├── convoy-sim/             # Vehicle simulator
│   └── src/main.rs         # HDDS publisher
└── convoy-viewer/          # Three.js viewer
    ├── index.html          # HUD layout
    ├── src/
    │   ├── main.js         # Three.js + WebSocket
    │   └── types.ts        # hdds_gen TypeScript output
    └── public/
        └── assets/models/  # escort-vehicle.glb, built by tools/
```

## Requirements

- Rust 1.75+
- Node.js 18+
- hdds-ws (from HDDS repo)
- idl-gen (from hdds_gen repo)

## 3D Assets

The vehicle mesh is assembled from boxes and prisms by
`tools/generate-vehicle-model.py`, which needs the standard library only
and writes the same bytes on every run. No third-party mesh ships with
this demo.

```bash
python3 tools/generate-vehicle-model.py
```

| Model | File | Source |
|-------|------|--------|
| Escort vehicle | `convoy-viewer/public/assets/models/escort-vehicle.glb` | Built in-repo, 288 triangles |

## License

Licensed under either of [Apache-2.0](../LICENSE-APACHE) or
[MIT](../LICENSE-MIT), at your option.
