//! Metal Slug Arena - Game Server
//!
//! Authoritative game server that manages:
//! - Player physics and collision
//! - Projectile simulation
//! - Power-up spawning
//! - Score tracking

use anyhow::Result;
use arena_types::arena::*;
use hdds::{Participant, QoS, TransportMode};
use log::*;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const TICK_RATE: u64 = 60;
const TICK_MS: u64 = 1000 / TICK_RATE;

// Game constants
const MAX_HEALTH: u8 = 100;
const PLAYER_SPEED: f32 = 200.0;
const JUMP_VELOCITY: f32 = -400.0;
const GRAVITY: f32 = 800.0;
const ARENA_WIDTH: f32 = 800.0;
const ARENA_HEIGHT: f32 = 600.0;
const PLAYER_WIDTH: f32 = 24.0;
const PLAYER_HEIGHT: f32 = 32.0;

// DDS topic names
mod topics {
    pub const PLAYER: &str = "arena/Player";
    pub const PROJECTILE: &str = "arena/Projectile";
    pub const EXPLOSION: &str = "arena/Explosion";
    pub const POWERUP: &str = "arena/PowerUp";
    pub const KILL: &str = "arena/KillEvent";
    pub const GAME_STATE: &str = "arena/GameState";
    pub const INPUT: &str = "arena/PlayerInput";
}

fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn weapon_name(w: WeaponType) -> &'static str {
    match w {
        WeaponType::PISTOL => "Pistol",
        WeaponType::WEAPON_HEAVY_MG => "Heavy MG",
        WeaponType::WEAPON_ROCKET => "Rocket",
        WeaponType::WEAPON_SHOTGUN => "Shotgun",
        WeaponType::LASER => "Laser",
        WeaponType::FLAME => "Flame",
        WeaponType::WEAPON_GRENADE => "Grenade",
    }
}

// Arena platforms (x, y, width, height)
const PLATFORMS: &[(f32, f32, f32, f32)] = &[
    (0.0, 560.0, 800.0, 40.0),
    (50.0, 450.0, 150.0, 20.0),
    (300.0, 400.0, 200.0, 20.0),
    (600.0, 450.0, 150.0, 20.0),
    (150.0, 300.0, 120.0, 20.0),
    (530.0, 300.0, 120.0, 20.0),
    (300.0, 200.0, 200.0, 20.0),
];

const SPAWN_POINTS: &[(f32, f32, Team)] = &[
    (100.0, 400.0, Team::RED),
    (700.0, 400.0, Team::BLUE),
    (200.0, 250.0, Team::RED),
    (600.0, 250.0, Team::BLUE),
    (400.0, 150.0, Team::NONE),
];

/// Player game state
#[derive(Clone)]
struct PlayerSim {
    player: Player,
    on_ground: bool,
    shoot_cooldown: f32,
    respawn_timer: f32,
    input_left: bool,
    input_right: bool,
    input_jump: bool,
    input_shoot: bool,
}

impl PlayerSim {
    fn new(id: u32, name: &str, t: Team, _rng: &mut impl Rng) -> Self {
        let spawn = SPAWN_POINTS.iter()
            .find(|(_, _, st)| *st == t || *st == Team::NONE)
            .unwrap_or(&SPAWN_POINTS[0]);

        PlayerSim {
            player: Player {
                player_id: id,
                name: name.to_string(),
                team: t,
                x: spawn.0,
                y: spawn.1,
                vx: 0.0,
                vy: 0.0,
                facing_right: t != Team::BLUE,
                state: PlayerState::IDLE,
                health: MAX_HEALTH,
                weapon: WeaponType::PISTOL,
                ammo: 0,
                kills: 0,
                deaths: 0,
                score: 0,
                timestamp_us: timestamp_us(),
            },
            on_ground: false,
            shoot_cooldown: 0.0,
            respawn_timer: 0.0,
            input_left: false,
            input_right: false,
            input_jump: false,
            input_shoot: false,
        }
    }

    fn respawn(&mut self, rng: &mut impl Rng) {
        let spawn_idx = rng.gen_range(0..SPAWN_POINTS.len());
        let spawn = &SPAWN_POINTS[spawn_idx];
        self.player.x = spawn.0;
        self.player.y = spawn.1;
        self.player.vx = 0.0;
        self.player.vy = 0.0;
        self.player.health = MAX_HEALTH;
        self.player.weapon = WeaponType::PISTOL;
        self.player.ammo = 0;
        self.player.state = PlayerState::IDLE;
        self.respawn_timer = 0.0;
    }
}

#[derive(Clone)]
struct ProjectileState {
    proj: Projectile,
    lifetime: f32,
}

struct PowerUpState {
    powerup: PowerUp,
    respawn_timer: f32,
}

struct PendingExplosion {
    x: f32,
    y: f32,
    size: ExplosionSize,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  METAL SLUG ARENA - Game Server");
    info!("  HDDS Multiplayer Demo");
    info!("==========================================");

    let participant = Participant::builder("arena_server")
        .domain_id(99)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    let player_writer = participant.create_writer::<Player>(topics::PLAYER, qos.clone())?;
    let proj_writer = participant.create_writer::<Projectile>(topics::PROJECTILE, qos.clone())?;
    let explosion_writer = participant.create_writer::<Explosion>(topics::EXPLOSION, qos.clone())?;
    let powerup_writer = participant.create_writer::<PowerUp>(topics::POWERUP, qos.clone())?;
    let kill_writer = participant.create_writer::<KillEvent>(topics::KILL, qos.clone())?;
    let state_writer = participant.create_writer::<GameState>(topics::GAME_STATE, qos.clone())?;
    let input_reader = participant.create_reader::<PlayerInput>(topics::INPUT, qos)?;

    info!("DDS participant ready");

    let mut rng = rand::thread_rng();
    let mut players: HashMap<u32, PlayerSim> = HashMap::new();
    let mut projectiles: HashMap<u32, ProjectileState> = HashMap::new();
    let mut powerups: Vec<PowerUpState> = Vec::new();
    let mut game_state = GameState {
        phase: GamePhase::PLAYING,
        time_remaining_s: 300,
        red_score: 0,
        blue_score: 0,
        player_count: 0,
        timestamp_us: timestamp_us(),
    };

    let mut next_projectile_id: u32 = 1;
    let mut next_explosion_id: u32 = 1;
    let mut next_powerup_id: u32 = 1;

    // Spawn power-ups
    let positions = [(100.0, 420.0), (400.0, 370.0), (700.0, 420.0), (200.0, 270.0), (600.0, 270.0), (400.0, 170.0)];
    for (x, y) in positions {
        let pt = [PowerUpType::HEALTH, PowerUpType::POWERUP_HEAVY_MG, PowerUpType::POWERUP_ROCKET, PowerUpType::POWERUP_SHOTGUN, PowerUpType::POWERUP_GRENADE][rng.gen_range(0..5)];
        powerups.push(PowerUpState {
            powerup: PowerUp { powerup_id: next_powerup_id, powerup_type: pt, x, y, active: true, timestamp_us: timestamp_us() },
            respawn_timer: 0.0,
        });
        next_powerup_id += 1;
    }

    // Add bot players
    for (id, name) in [(1001, "Marco"), (1002, "Tarma"), (1003, "Eri"), (1004, "Fio")] {
        let t = if players.values().filter(|p| p.player.team == Team::RED).count()
            <= players.values().filter(|p| p.player.team == Team::BLUE).count() { Team::RED } else { Team::BLUE };
        players.insert(id, PlayerSim::new(id, name, t, &mut rng));
        info!("Bot {} joined team {}", name, if t == Team::RED { "RED" } else { "BLUE" });
    }
    game_state.player_count = players.len() as u8;

    let mut last_tick = Instant::now();
    let mut tick_count: u64 = 0;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Process inputs
        while let Ok(Some(input)) = input_reader.take() {
            if let Some(state) = players.get_mut(&input.player_id) {
                state.input_left = input.left;
                state.input_right = input.right;
                state.input_jump = input.jump;
                state.input_shoot = input.shoot;
            }
        }

        // Bot AI
        for (id, state) in &mut players {
            if *id >= 1000 {
                state.input_left = rng.gen_bool(0.3);
                state.input_right = rng.gen_bool(0.3);
                state.input_jump = rng.gen_bool(0.02);
                state.input_shoot = rng.gen_bool(0.05);
            }
        }

        // Update players and collect shots
        let mut new_projectiles: Vec<Projectile> = Vec::new();
        let player_ids: Vec<u32> = players.keys().cloned().collect();

        for id in &player_ids {
            let state = players.get_mut(id).unwrap();

            if state.player.state == PlayerState::DEAD {
                state.respawn_timer -= dt;
                if state.respawn_timer <= 0.0 {
                    state.respawn(&mut rng);
                }
                continue;
            }

            // Movement
            state.player.vx = 0.0;
            if state.input_left {
                state.player.vx = -PLAYER_SPEED;
                state.player.facing_right = false;
            }
            if state.input_right {
                state.player.vx = PLAYER_SPEED;
                state.player.facing_right = true;
            }

            if state.input_jump && state.on_ground {
                state.player.vy = JUMP_VELOCITY;
                state.on_ground = false;
            }

            state.player.vy += GRAVITY * dt;
            state.player.x += state.player.vx * dt;
            state.player.y += state.player.vy * dt;

            // Platform collision
            state.on_ground = false;
            for &(px, py, pw, ph) in PLATFORMS {
                if state.player.vy > 0.0 {
                    let bottom = state.player.y + PLAYER_HEIGHT;
                    let cx = state.player.x + PLAYER_WIDTH / 2.0;
                    if cx > px && cx < px + pw && bottom >= py && bottom <= py + ph + state.player.vy * dt {
                        state.player.y = py - PLAYER_HEIGHT;
                        state.player.vy = 0.0;
                        state.on_ground = true;
                    }
                }
            }

            state.player.x = state.player.x.clamp(0.0, ARENA_WIDTH - PLAYER_WIDTH);
            if state.player.y > ARENA_HEIGHT {
                state.player.health = 0;
                state.player.state = PlayerState::DEAD;
                state.player.deaths += 1;
                state.respawn_timer = 2.0;
                continue;
            }

            // Shooting
            state.shoot_cooldown -= dt;
            if state.input_shoot && state.shoot_cooldown <= 0.0 {
                let weapon = state.player.weapon;
                if weapon != WeaponType::PISTOL && state.player.ammo == 0 {
                    state.player.weapon = WeaponType::PISTOL;
                } else {
                    if weapon != WeaponType::PISTOL {
                        state.player.ammo -= 1;
                    }

                    let (cooldown, speed, damage) = match weapon {
                        WeaponType::WEAPON_HEAVY_MG => (0.08, 800.0, 10),
                        WeaponType::WEAPON_ROCKET => (0.8, 400.0, 50),
                        WeaponType::WEAPON_SHOTGUN => (0.5, 500.0, 25),
                        WeaponType::LASER => (0.1, 1200.0, 12),
                        WeaponType::FLAME => (0.05, 300.0, 5),
                        _ => (0.3, 600.0, 15),
                    };

                    state.shoot_cooldown = cooldown;
                    let dir = if state.player.facing_right { 1.0 } else { -1.0 };
                    let sx = state.player.x + PLAYER_WIDTH / 2.0 + dir * 20.0;
                    let sy = state.player.y + PLAYER_HEIGHT / 2.0 - 5.0;

                    let pellets = if weapon == WeaponType::WEAPON_SHOTGUN { 5 } else { 1 };
                    for i in 0..pellets {
                        let spread = if weapon == WeaponType::WEAPON_SHOTGUN { (i as f32 - 2.0) * 0.15 } else { 0.0 };
                        new_projectiles.push(Projectile {
                            projectile_id: next_projectile_id,
                            owner_id: *id,
                            weapon,
                            x: sx,
                            y: sy,
                            vx: dir * speed,
                            vy: spread * speed,
                            damage,
                            active: true,
                            timestamp_us: timestamp_us(),
                        });
                        next_projectile_id += 1;
                    }
                }
            }

            state.player.state = if state.player.vy < -10.0 { PlayerState::JUMPING }
                else if state.player.vy > 10.0 { PlayerState::FALLING }
                else if state.player.vx.abs() > 10.0 { PlayerState::RUNNING }
                else { PlayerState::IDLE };

            state.player.timestamp_us = timestamp_us();
        }

        // Add new projectiles
        for proj in new_projectiles {
            projectiles.insert(proj.projectile_id, ProjectileState { proj, lifetime: 3.0 });
        }

        // Update projectiles
        let mut kills: Vec<KillEvent> = Vec::new();
        let mut pending_explosions: Vec<PendingExplosion> = Vec::new();
        let proj_ids: Vec<u32> = projectiles.keys().cloned().collect();

        for proj_id in proj_ids {
            let pstate = projectiles.get_mut(&proj_id).unwrap();
            pstate.proj.x += pstate.proj.vx * dt;
            pstate.proj.y += pstate.proj.vy * dt;
            if pstate.proj.weapon == WeaponType::WEAPON_ROCKET {
                pstate.proj.vy += GRAVITY * 0.3 * dt;
            }
            pstate.lifetime -= dt;

            if pstate.lifetime <= 0.0 || pstate.proj.x < -50.0 || pstate.proj.x > 850.0 || pstate.proj.y < -50.0 || pstate.proj.y > 650.0 {
                pstate.proj.active = false;
                continue;
            }

            // Platform collision for rockets
            if pstate.proj.weapon == WeaponType::WEAPON_ROCKET {
                for &(px, py, pw, ph) in PLATFORMS {
                    if pstate.proj.x >= px && pstate.proj.x <= px + pw && pstate.proj.y >= py && pstate.proj.y <= py + ph {
                        pstate.proj.active = false;
                        pending_explosions.push(PendingExplosion { x: pstate.proj.x, y: pstate.proj.y, size: ExplosionSize::LARGE });
                        break;
                    }
                }
            }

            if !pstate.proj.active { continue; }

            // Player collision
            let owner_id = pstate.proj.owner_id;
            let damage = pstate.proj.damage;
            let weapon = pstate.proj.weapon;
            let (px, py) = (pstate.proj.x, pstate.proj.y);

            for (player_id, psim) in &mut players {
                if *player_id == owner_id || psim.player.state == PlayerState::DEAD { continue; }

                if px >= psim.player.x && px <= psim.player.x + PLAYER_WIDTH
                    && py >= psim.player.y && py <= psim.player.y + PLAYER_HEIGHT
                {
                    let old_health = psim.player.health;
                    psim.player.health = psim.player.health.saturating_sub(damage);
                    pstate.proj.active = false;
                    pending_explosions.push(PendingExplosion { x: px, y: py, size: ExplosionSize::SMALL });

                    if psim.player.health == 0 && old_health > 0 {
                        psim.player.state = PlayerState::DEAD;
                        psim.player.deaths += 1;
                        psim.respawn_timer = 3.0;

                        let victim_name = psim.player.name.clone();
                        let victim_id = *player_id;

                        if let Some(killer) = players.get_mut(&owner_id) {
                            killer.player.kills += 1;
                            killer.player.score += 100;
                            if killer.player.team == Team::RED { game_state.red_score += 1; }
                            else if killer.player.team == Team::BLUE { game_state.blue_score += 1; }

                            pending_explosions.push(PendingExplosion { x: px, y: py, size: ExplosionSize::MEDIUM });

                            kills.push(KillEvent {
                                killer_id: owner_id,
                                victim_id,
                                weapon,
                                killer_name: killer.player.name.clone(),
                                victim_name,
                                timestamp_us: timestamp_us(),
                            });
                        }
                    }
                    break;
                }
            }
            pstate.proj.timestamp_us = timestamp_us();
        }

        projectiles.retain(|_, p| p.proj.active);

        // Update power-ups
        for powerup in &mut powerups {
            if !powerup.powerup.active {
                powerup.respawn_timer -= dt;
                if powerup.respawn_timer <= 0.0 {
                    powerup.powerup.active = true;
                    powerup.powerup.powerup_type = [PowerUpType::HEALTH, PowerUpType::POWERUP_HEAVY_MG, PowerUpType::POWERUP_ROCKET, PowerUpType::POWERUP_SHOTGUN, PowerUpType::POWERUP_GRENADE][rng.gen_range(0..5)];
                    powerup.powerup.timestamp_us = timestamp_us();
                }
            }
        }

        // Power-up pickups
        for powerup in &mut powerups {
            if !powerup.powerup.active { continue; }
            for psim in players.values_mut() {
                if psim.player.state == PlayerState::DEAD { continue; }
                let dx = (psim.player.x + PLAYER_WIDTH / 2.0) - powerup.powerup.x;
                let dy = (psim.player.y + PLAYER_HEIGHT / 2.0) - powerup.powerup.y;
                if (dx * dx + dy * dy).sqrt() < 30.0 {
                    match powerup.powerup.powerup_type {
                        PowerUpType::HEALTH => { psim.player.health = MAX_HEALTH; }
                        PowerUpType::POWERUP_HEAVY_MG => { psim.player.weapon = WeaponType::WEAPON_HEAVY_MG; psim.player.ammo = 100; }
                        PowerUpType::POWERUP_ROCKET => { psim.player.weapon = WeaponType::WEAPON_ROCKET; psim.player.ammo = 10; }
                        PowerUpType::POWERUP_SHOTGUN => { psim.player.weapon = WeaponType::WEAPON_SHOTGUN; psim.player.ammo = 20; }
                        PowerUpType::POWERUP_GRENADE => { psim.player.weapon = WeaponType::WEAPON_GRENADE; psim.player.ammo = 5; }
                        _ => {}
                    }
                    powerup.powerup.active = false;
                    powerup.respawn_timer = 10.0;
                    break;
                }
            }
        }

        // Publish state via DDS
        for state in players.values() {
            player_writer.write(&state.player)?;
        }

        for state in projectiles.values() {
            proj_writer.write(&state.proj)?;
        }

        for exp in &pending_explosions {
            let explosion = Explosion {
                explosion_id: next_explosion_id,
                x: exp.x,
                y: exp.y,
                size: match exp.size { ExplosionSize::SMALL => 15.0, ExplosionSize::MEDIUM => 30.0, _ => 50.0 },
                explosion_type: exp.size,
                timestamp_us: timestamp_us(),
            };
            next_explosion_id += 1;
            explosion_writer.write(&explosion)?;
        }

        for powerup in &powerups {
            powerup_writer.write(&powerup.powerup)?;
        }

        for kill in &kills {
            kill_writer.write(kill)?;
            info!("{} killed {} with {}", kill.killer_name, kill.victim_name, weapon_name(kill.weapon));
        }

        if tick_count % TICK_RATE == 0 {
            game_state.timestamp_us = timestamp_us();
            state_writer.write(&game_state)?;
        }

        tick_count += 1;

        let elapsed = last_tick.elapsed();
        if elapsed < Duration::from_millis(TICK_MS) {
            tokio::time::sleep(Duration::from_millis(TICK_MS) - elapsed).await;
        }
    }
}
