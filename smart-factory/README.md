# Smart Factory

HDDS demo of an industrial production line with isometric 2.5D visualization.

## Features

- **Production Line**: 6 machines (CNC, Welder, Assembler, Painter, Inspector, Packager)
- **Conveyor Belts**: Animated belts connecting machines
- **Products**: Items moving through the production stages
- **Telemetry**: Temperature, vibration, power consumption
- **Alerts**: Fault detection and notifications
- **Isometric View**: 2.5D factory floor visualization

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p factory-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd factory-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Architecture

```
┌─────────────────────────┐      DDS       ┌──────────┐  WebSocket  ┌─────────────────┐
│      factory-sim        │ ─────────────►│  hdds-ws  │───────────►│     Browser     │
│     (simulator)         │               │  (bridge) │            │  (Isometric 2D) │
│                         │               └──────────┘            │                 │
│  - Machine states       │                                       │  - Factory floor│
│  - Product flow         │                                       │  - Machines     │
│  - Fault simulation     │                                       │  - Conveyors    │
│  - DDS publisher        │                                       │  - Products     │
└─────────────────────────┘                                       └─────────────────┘
```

## DDS Topics

| Topic | Type | Description |
|-------|------|-------------|
| `factory/machine` | Machine | Machine status and telemetry |
| `factory/conveyor` | Conveyor | Conveyor belt state |
| `factory/product` | Product | Product position and quality |
| `factory/alert` | Alert | Fault alerts |
| `factory/factory_state` | FactoryState | Overall statistics |

## Production Line Layout

```
(0,0) CNC ──► (2,0) WELDER ──► (4,0) ASSEMBLER
                                      │
                                      ▼
(0,2) PACKAGER ◄── (2,2) QC ◄── (4,2) PAINTER
```

## Machine Types

| Type | Function |
|------|----------|
| CNC | Machining raw materials |
| WELDER | Joining components |
| ASSEMBLER | Assembly station |
| PAINTER | Paint booth |
| INSPECTOR | Quality control |
| PACKAGER | Final packaging |

## Machine States

| State | Description | Color |
|-------|-------------|-------|
| IDLE | No work | Gray |
| WORKING | Processing | Cyan |
| BLOCKED | Output full | Yellow |
| STARVED | No input | Yellow |
| MAINTENANCE | Scheduled | Purple |
| FAULT | Error | Red (blinking) |

## IDL Types

Generated from `idl/factory.idl` using hdds_gen:

```idl
module factory {
    enum MachineType { MACHINE_CNC, MACHINE_WELDER, ... };
    enum MachineState { STATE_IDLE, STATE_WORKING, STATE_FAULT, ... };

    @topic struct Machine {
        @key uint32 machine_id;
        string<32> name;
        MachineType machine_type;
        MachineState state;
        float temperature_c;
        float vibration_mm_s;
        float power_kw;
        // ...
    };
}
```

## Dependencies

- Rust + Cargo
- Node.js 18+
- HDDS (workspace dependency)
