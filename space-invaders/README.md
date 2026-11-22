# Space Invaders DDS

Classic arcade game with DDS-based game state synchronization.

## Features

- **Classic Gameplay**: Defend Earth against waves of aliens
- **3 Alien Types**: Basic (10pts), Fast (20pts), Heavy (30pts, 2 hits)
- **Power-ups**: Rapid Fire, Shield, Triple Shot
- **DDS Sync**: All game state published via HDDS topics
- **Pixel Art**: Custom sprites with animations

## Quick Start

```bash
# Terminal 1: Start the game server
cargo run -p invaders-sim

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer
cd invaders-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To play without the backend, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Controls

**These drive the ship in demo mode only.** Against the live simulator the
viewer publishes its keys to `invaders/player_input`, but `invaders-sim`
creates no reader for that topic, so the ship is flown by the simulation and
the keys change nothing. The IDL already declares the `PlayerInput` type;
wiring it end to end also needs a typed publish path through the bridge, which
today encodes only a single string member.

| Key | Action |
|-----|--------|
| ← → | Move left/right |
| SPACE | Fire |
| R | Restart game |

## Architecture

```
┌─────────────────────────┐      DDS       ┌──────────┐  WebSocket  ┌─────────────────┐
│      invaders-sim       │ ─────────────►│  hdds-ws  │◄──────────►│     Browser     │
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
| `invaders/player` | Player | Player position, lives, score, power-up |
| `invaders/alien` | Alien[] | All aliens state (position, health, type) |
| `invaders/bullet` | Bullet[] | Active bullets (player and alien) |
| `invaders/explosion` | Explosion[] | Active explosion effects |
| `invaders/powerup` | PowerUp[] | Dropped power-ups |
| `invaders/game_state` | GameState | Wave, phase, high score |
| `invaders/kill_event` | KillEvent | Kill notifications for effects |

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
