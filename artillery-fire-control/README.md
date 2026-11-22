# Artillery Fire Control Demo

HDDS demonstration showing a tactical artillery fire control system with:
- 3 artillery batteries on a 20km x 20km tactical map
- Fire missions from forward observers with targeting requests
- Ballistic computation (azimuth, elevation, time-of-flight)
- Real-time round tracking with physics (drag, gravity, wind)
- Forward observer correction loop (CEP convergence)
- Live weather simulation affecting ballistic accuracy

## Quick Start

```bash
# Terminal 1: Start the simulator
cargo run -p arty-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer (dev server)
cd arty-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Architecture

```
+--------------+      DDS UDP      +--------------+    WebSocket    +--------------+
|   arty-sim   | ----------------> |   hdds-ws    | -------------> |   Browser    |
| (simulator)  |                   | (WS bridge)  |                |  (Canvas 2D) |
+--------------+                   +--------------+                +--------------+
      |
      | Publishes:
      |-- BatteryStatus     (1 Hz, RELIABLE + TRANSIENT_LOCAL)
      |-- FireMission        (event, RELIABLE)
      |-- BallisticSolution  (event, RELIABLE)
      |-- RoundInFlight      (10 Hz, BEST_EFFORT)
      |-- ImpactObservation  (event, RELIABLE)
      +-- MeteoData          (0.5 Hz, BEST_EFFORT)
```

## Topics

| Topic | Type | QoS | Rate | Description |
|-------|------|-----|------|-------------|
| `arty/battery` | BatteryStatus | RELIABLE + TRANSIENT_LOCAL | 1 Hz | Battery position, ammo, barrel temp, state |
| `arty/mission` | FireMission | RELIABLE | Event | Fire request from FO (target coords, round type) |
| `arty/solution` | BallisticSolution | RELIABLE | Event | Computed firing solution (az/el/ToF/range) |
| `arty/round` | RoundInFlight | BEST_EFFORT | 10 Hz | Round position/velocity during flight |
| `arty/impact` | ImpactObservation | RELIABLE | Event | FO correction (range/deflection error) |
| `arty/meteo` | MeteoData | BEST_EFFORT | 0.5 Hz | Wind, temperature, pressure, humidity |

## Mission Lifecycle

```
New target                                     Target destroyed
    |                                               |
    v                                               v
PENDING --> COMPUTING --> MISSION_FIRING --> ADJUSTING --> COMPLETE
               (2s)      (rounds     (3s FO        ^
                          in air)    observes)      |
                                        |           |
                                        +-----------+
                                      correction loop
                                      (CEP * 0.6 each pass)
```

## Ballistic Model

- Muzzle velocity: 560 m/s (M777 charge 5)
- Base CEP: 120m, improves 40% per FO correction
- 3D physics with drag (0.0001 * speed) and gravity (9.81 m/s2)
- Wind drift applied from live meteo data

## Viewer Features

- **Tactical Map**: 20km x 20km grid with 2km spacing
- **Batteries**: Triangle symbols, color-coded by state (ready/loading/firing/cooldown)
- **Targets**: Crosshair markers with impact rings
- **Trajectories**: Dashed flight paths with 10s fade
- **Rounds**: Yellow dots sized by altitude with glow effect
- **Impacts**: Expanding rings (orange=miss, green=destroyed)
- **Wind Compass**: Arrow showing wind direction and speed
- **Sidebars**: Mission list, battery panel, meteo data, impact log

## Project Structure

```
artillery-fire-control/
  idl/artillery.idl          IDL type definitions (3 enums, 6 structs)
  arty-types/                Generated Rust types + CDR codecs
  arty-sim/                  Simulation engine (10 Hz main loop)
  arty-viewer/               Web UI (Vite + Canvas 2D)
```

## Dependencies

- Rust + Cargo (for simulator)
- Node.js 18+ (for viewer dev server)
- HDDS (included via workspace)
- hdds-ws (WebSocket bridge)
