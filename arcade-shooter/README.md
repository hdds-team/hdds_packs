# Arcade Shooter DDS

Wave shooter with DDS-based game state synchronization.

## Features

- **Classic Gameplay**: Defend Earth against waves of aliens
- **3 Alien Types**: Basic (10pts), Fast (20pts), Heavy (30pts, 2 hits)
- **Power-ups**: Rapid Fire, Shield, Triple Shot
- **DDS Sync**: All game state published via HDDS topics
- **Pixel Art**: Custom sprites with animations

## Quick Start

```bash
# Terminal 1: Start the game server
cargo run -p shooter-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd shooter-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To play without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Controls

| Key | Action |
|-----|--------|
| ← → | Move left/right |
| SPACE | Fire |
| R | Restart the offline demo |

The arrows and SPACE drive the ship against the live simulator: the viewer
publishes a `PlayerInput` sample on `shooter/player_input` at 60 Hz and
`shooter-sim` reads it. The viewer encodes that sample itself, because the
bridge's JSON publish path writes a single string member, which a typed reader
cannot decode; it hands the bytes over in a `_raw` field instead.

`R` restarts the offline demo only. `PlayerInput` carries a direction and a
fire flag, so there is nothing on the wire to ask the simulator for a restart.

## Architecture

```
┌─────────────────────────┐      DDS       ┌──────────┐  WebSocket  ┌─────────────────┐
│       shooter-sim       │ ─────────────►│  hdds-ws  │◄──────────►│     Browser     │
│    (game logic)         │◄───────────── │  (bridge) │            │  (Canvas 2D)    │
│                         │               └──────────┘            │                 │
│  - Alien AI             │                                       │  - Sprites      │
│  - Collision detection  │                                       │  - Animations   │
│  - Power-up spawning    │                                       │  - Score/HUD    │
│  - Wave progression     │                                       │  - Starfield    │
└─────────────────────────┘                                       └─────────────────┘
```

## DDS Topics

| Topic | Type | Description |
|-------|------|-------------|
| `shooter/player` | Player | Player position, lives, score, power-up |
| `shooter/alien` | Alien[] | All aliens state (position, health, type) |
| `shooter/bullet` | Bullet[] | Active bullets (player and alien) |
| `shooter/explosion` | Explosion[] | Active explosion effects |
| `shooter/powerup` | PowerUp[] | Dropped power-ups |
| `shooter/game_state` | GameState | Wave, phase, high score |
| `shooter/kill_event` | KillEvent | Kill notifications for effects |

## Sprites

Sprites are in `assets/sprites/`:
- `player.png` - Player ship
- `alien1.png`, `alien2.png`, `alien3.png` - Alien types
- `bullet_player.png`, `bullet_alien.png` - Projectiles
- `explosion1-4.png` - Explosion animation frames
- `powerup.png` - Power-up pickup
- `boss.png` - Boss alien (future use)

## Game Mechanics

### Aliens
- Formation: 5 rows × 11 columns (55 aliens)
- Top row: Heavy (30pts, 2 HP)
- Middle rows: Fast (20pts)
- Bottom rows: Basic (10pts)
- Speed increases as aliens are destroyed

### Power-ups
- 10% drop chance on kill
- Duration: 10 seconds
- **Rapid Fire**: 3x fire rate
- **Shield**: Absorbs one hit
- **Triple Shot**: 3 bullets per shot

### Waves
- Wave clear → 2 second pause → next wave
- Each wave: +20% alien speed

## Dependencies

- Rust + Cargo
- Node.js 18+
- HDDS (workspace dependency)
