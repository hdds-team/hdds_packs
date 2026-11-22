# F1Tenth Autonomous Racing

4 autonomous F1Tenth-scale cars racing on a closed circuit.
Real-time telemetry via HDDS DDS, Three.js 3D viewer.

## Architecture

3 processus independants, relies uniquement par DDS :

```
f1tenth-sim (Rust)          hdds-ws (bridge)          f1tenth-viewer (Three.js)
      |                          |                          |
      |  DDS topics (CDR2)       |  JSON / WebSocket        |
      |--- f1tenth/car -------->|--- { sample: {...} } --->|
      |--- f1tenth/track_point >|                          |
      |--- f1tenth/race_state ->|                          |
      |--- f1tenth/lap_event -->|                          |
      |--- f1tenth/sector_time >|                          |
      |                          |                          |
      60Hz physics              auto-decode XTypes         Three.js 60fps
      domain 99                 port 9090                  port 5173
```

Le sim ne sait rien du viewer et vice-versa. Tout passe par DDS.
On peut brancher un 2e viewer, un logger, un `hdds-topic-echo` sans rien changer au sim.

## Topics DDS

| Topic | Type | Rate | Description |
|-------|------|------|-------------|
| `f1tenth/car` | Car | 60Hz x 4 cars | Position, vitesse, heading, progression |
| `f1tenth/track_point` | TrackPoint | once (burst) | Points du circuit + largeur |
| `f1tenth/race_state` | RaceState | 2Hz | Phase de course, countdown, chrono |
| `f1tenth/lap_event` | LapEvent | event | Tour complete + temps |
| `f1tenth/sector_time` | SectorTime | event | Temps par secteur |

Debit total : ~240 msg/s en course (4 cars x 60Hz).

## Quick Start

```bash
# 1. Build the sim
cargo build --release -p f1tenth-sim

# 2. Start hdds-ws bridge (domain 99)
hdds-ws --domain 99 --port 9090

# 3. Start the viewer
cd f1tenth-viewer && npm install && npm run dev

# 4. Run the sim
cargo run --release -p f1tenth-sim

# 5. Open http://localhost:5173
#    (or http://localhost:5173?demo for offline demo mode)
```

Pour observer les topics depuis le terminal :

```bash
hdds-topic-echo f1tenth/car --domain 99
hdds-topic-echo f1tenth/lap_event --domain 99
```

## Sim : modele physique

### Modele bicyclette

Chaque voiture utilise un modele bicyclette simplifie (2 roues, pas de modele de pneu) :

```
      [front wheel]
           |  steering_angle
           |
     WHEELBASE (0.33m)
           |
      [rear wheel]
```

- **Wheelbase** : 0.33m (standard F1Tenth)
- **Vitesse max** : 8 m/s (~29 km/h)
- **Braquage max** : 0.42 rad (~24 deg)
- **Rayon de virage** : `R = wheelbase / tan(steering_angle)`
- **Vitesse angulaire** : `omega = speed / R`
- **Drag** lineaire (0.3)

### IA waypoint-following

Chaque voiture suit la meme logique :

1. **Lookahead** : calculer un point cible sur le circuit, plus loin a haute vitesse
   (`lookahead = 0.8m + speed * 0.15`)
2. **Heading error** : angle entre le cap actuel et la direction du point cible
3. **Steering** : proportionnel a l'erreur (`steer = heading_err * 2.5 * skill`)
4. **Throttle** : plein gaz en ligne droite, freinage en courbe

Le freinage utilise l'estimation de courbure de Menger (3 points du circuit) :

```
courbure = 4 * aire(triangle) / (d01 * d12 * d02)
vitesse_max_virage = sqrt(grip / courbure)
```

### Personnalites

Chaque voiture a deux parametres randomises au demarrage :

| Param | Effet |
|-------|-------|
| `skill` (0.91-0.98) | Multiplie vitesse max et precision du braquage |
| `aggression` (0.55-0.95) | Affecte le point de freinage (plus tard = plus agressif) |

Resultat : APEX est generalement en tete (~9.9s/tour), SHADOW plus prudent (~10.5s/tour).

## Circuit

Le circuit est defini par 20 control points interpoles (x5 = 100 points).
Longueur totale : ~36m. 3 secteurs.

```
     Start/Finish
     v
(0,0)-----(6,0)
                \
                 (10,4)    Turn 1
                 |
                 (10,6)
                /
    Chicane   (8,9.5)
              /
         (5,10.5)
        /
   (2,8.5)              Turn 3 (hairpin)
   |
   (1.5,7)
   |
   (2,5.5)
    \
     (3.5,3.5)
      \
       (3,2)---(0.5,0.2)---> boucle
```

## Viewer Three.js

### Scene 3D
- Sol + grille
- Circuit rendu comme un ruban 3D (track points + normales + largeur)
- Ligne de depart (marqueur blanc)
- Voitures : body (box), nez, aileron arriere, 4 roues (cylindres), numero (sprite)
- Trainee par voiture (600 points = ~10s a 60Hz)
- Lumiere directionnelle avec ombres

### HUD
- Chrono de course
- Leaderboard live (position, couleur, nom, ecart, tour)
- Info voiture suivie (vitesse, tour, best lap, last lap)
- Indicateurs de secteur (S1/S2/S3)
- Minimap canvas 2D

### Camera
- **Follow** (touches 1-4) : camera derriere la voiture suivie, smooth lerp
- **Free** (touche 0) : orbite autour du centre du circuit
- **Zoom** : molette souris

### Mode demo
`http://localhost:5173?demo` : 4 voitures sur un ovale, pas besoin de sim ni de hdds-ws.

## Cars

| # | Name | Color | Skill | Aggression | Personnalite |
|---|------|-------|-------|------------|--------------|
| 1 | APEX | #FF2200 (red) | 0.97 | 0.85 | Rapide, agressif |
| 2 | PHANTOM | #00CCFF (cyan) | 0.95 | 0.70 | Regulier, smooth |
| 3 | VORTEX | #FFCC00 (yellow) | 0.92 | 0.90 | Imprevisible |
| 4 | SHADOW | #AA00FF (purple) | 0.93 | 0.60 | Prudent, constant |

Les valeurs de skill/aggression sont perturbees aleatoirement a chaque course (+/- 0.02/0.05).

## Structure

```
f1tenth-racing/
  Cargo.toml              # Workspace
  README.md
  idl/
    f1tenth.idl           # Types IDL 4.2 (source de verite)
  f1tenth-types/          # Types Rust (genere par hddsgen)
    Cargo.toml
    src/lib.rs
  f1tenth-sim/            # Simulateur (physique + IA + DDS publish)
    Cargo.toml
    src/main.rs
  f1tenth-viewer/         # Frontend web (Three.js + hdds-ws)
    package.json
    vite.config.js
    index.html
    src/
      main.js             # Scene 3D, WebSocket, HUD
      style.css           # HUD styling
      types.ts            # Types TS (genere par hddsgen)
```

## Regenerer les types

Si le IDL change :

```bash
# Rust
hddsgen gen rust idl/f1tenth.idl \
  --hdds-path /path/to/hdds/crates/hdds \
  --serde \
  -o f1tenth-types/src/lib.rs

# TypeScript
hddsgen gen typescript idl/f1tenth.idl \
  -o f1tenth-viewer/src/types.ts
```

## Resultats typiques

```
LIGHTS OUT AND AWAY WE GO!
APEX completed lap 1 (10.511s BEST)
PHANTOM completed lap 1 (10.820s BEST)
...
APEX completed lap 5 (9.840s BEST)
CHECKERED FLAG!
  P1 APEX    - 49.9s (best lap: 9.840s)
  P2 PHANTOM - 52.1s (best lap: 10.296s)
  P3 VORTEX  - 52.1s (best lap: 10.290s)
  P4 SHADOW  - 53.1s (best lap: 10.479s)
```

Les tours s'ameliorent au fil de la course (les voitures trouvent la trajectoire).
Le classement varie selon la randomisation initiale de skill/aggression.
