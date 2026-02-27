# Metal Slug Arena

HDDS multiplayer 2D arena combat game inspired by Metal Slug!

## Features

- **Multiplayer**: Real-time combat via DDS
- **Teams**: Red vs Blue team deathmatch
- **Weapons**: Pistol, Heavy MG, Rocket, Shotgun, Laser, Flamethrower
- **Power-ups**: Health, weapon pickups
- **Physics**: Platformer movement with gravity and collision
- **Retro Style**: Pixel art aesthetic with arcade HUD

## Quick Start

```bash
# Terminal 1: Start the game server
cargo run -p arena-server

# Terminal 2: Start the WebSocket bridge
hdds-ws --domain 99 --port 9090

# Terminal 3: Start the viewer (dev server)
cd arena-viewer && npm install && npm run dev
```

Then open http://localhost:5173 in your browser.

## Demo Mode (No Backend)

To run the viewer without the game server, add `?demo=1` to the URL:
http://localhost:5173/?demo=1

## Controls

| Key | Action |
|-----|--------|
| Arrow Left / A | Move left |
| Arrow Right / D | Move right |
| Arrow Up / W / Space | Jump |
| X / Z / Ctrl | Shoot |

## Architecture

```
┌─────────────────┐     DDS UDP      ┌─────────────────┐
│  arena-server   │ ◄──────────────► │     Browser     │
│  (game logic)   │                  │  (arena-viewer) │
│                 │    WebSocket     │                 │
│  - Physics      │ ────────────────►│  - Canvas 2D    │
│  - Collision    │                  │  - Input        │
│  - Weapons      │                  │  - Rendering    │
│  - Power-ups    │                  │  - HUD          │
└─────────────────┘                  └─────────────────┘
```

## Topics

| Topic | Description | Direction |
|-------|-------------|-----------|
| `arena/players` | Player positions and states | Server → Client |
| `arena/inputs` | Player keyboard inputs | Client → Server |
| `arena/projectiles` | Bullets and rockets | Server → Client |
| `arena/explosions` | Explosion effects | Server → Client |
| `arena/powerups` | Power-up locations | Server → Client |
| `arena/kills` | Kill events | Server → Client |
| `arena/state` | Game state (score, time) | Server → Client |

## Weapons

| Weapon | Damage | Fire Rate | Ammo |
|--------|--------|-----------|------|
| Pistol | 15 | 0.3s | ∞ |
| Heavy MG | 10 | 0.08s | 100 |
| Rocket | 50 | 0.8s | 10 |
| Shotgun | 25×5 | 0.5s | 20 |
| Laser | 12 | 0.1s | 50 |
| Flame | 5 | 0.05s | 100 |

## Bot Players

The server spawns 4 AI bot players (Marco, Tarma, Eri, Fio) for testing.
They perform random actions to simulate combat.

## Dependencies

- Rust + Cargo (for game server)
- Node.js 18+ (for viewer dev server)
- HDDS (included via workspace)
