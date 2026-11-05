//! Space Invaders DDS Simulator
//!
//! Classic arcade game with DDS-based state synchronization.

use anyhow::Result;
use invaders_types::invaders::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// Game constants
const GAME_WIDTH: f32 = 800.0;
const GAME_HEIGHT: f32 = 600.0;
const PLAYER_Y: f32 = 550.0;
const PLAYER_SPEED: f32 = 300.0;
const BULLET_SPEED: f32 = 500.0;
const ALIEN_BULLET_SPEED: f32 = 200.0;
const ALIEN_ROWS: u8 = 5;
const ALIEN_COLS: u8 = 11;
const ALIEN_START_X: f32 = 100.0;
const ALIEN_START_Y: f32 = 80.0;
const ALIEN_SPACING_X: f32 = 50.0;
const ALIEN_SPACING_Y: f32 = 40.0;

/// Topic names
mod topics {
    pub const PLAYER: &str = "invaders/player";
    pub const ALIEN: &str = "invaders/alien";
    pub const BULLET: &str = "invaders/bullet";
    pub const EXPLOSION: &str = "invaders/explosion";
    pub const POWERUP: &str = "invaders/powerup";
    pub const GAME_STATE: &str = "invaders/game_state";
    pub const KILL_EVENT: &str = "invaders/kill_event";
}

/// Current time in microseconds
fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

/// Game simulation state
struct GameSim {
    // Entities
    player: Player,
    aliens: Vec<Alien>,
    bullets: Vec<Bullet>,
    explosions: Vec<Explosion>,
    powerups: Vec<PowerUp>,

    // Game state
    game_state: GameState,

    // Movement
    alien_direction: f32,
    alien_move_timer: f32,

    // IDs
    next_bullet_id: u32,
    next_explosion_id: u32,
    next_powerup_id: u32,
    next_event_id: u32,

    // Input
    input_move: i8,
    input_fire: bool,
    fire_cooldown: f32,

    // Events to broadcast
    kill_events: Vec<KillEvent>,
}

impl GameSim {
    fn new() -> Self {
        let mut sim = Self {
            player: Player {
                player_id: 1,
                x: GAME_WIDTH / 2.0,
                y: PLAYER_Y,
                lives: 3,
                score: 0,
                power_up: PowerUpType::POWER_NONE,
                power_up_time: 0.0,
                alive: true,
                timestamp_us: now_us(),
            },
            aliens: Vec::new(),
            bullets: Vec::new(),
            explosions: Vec::new(),
            powerups: Vec::new(),
            game_state: GameState {
                phase: GamePhase::PHASE_PLAYING,
                current_wave: 1,
                aliens_remaining: 0,
                aliens_total: 0,
                alien_speed: 1.0,
                high_score: 0,
                timestamp_us: now_us(),
            },
            alien_direction: 1.0,
            alien_move_timer: 0.0,
            next_bullet_id: 1,
            next_explosion_id: 1,
            next_powerup_id: 1,
            next_event_id: 1,
            input_move: 0,
            input_fire: false,
            fire_cooldown: 0.0,
            kill_events: Vec::new(),
        };
        sim.spawn_wave(1);
        sim
    }

    fn spawn_wave(&mut self, wave: u8) {
        self.aliens.clear();
        self.bullets.clear();

        let mut alien_id = 1u32;

        for row in 0..ALIEN_ROWS {
            for col in 0..ALIEN_COLS {
                let alien_type = match row {
                    0 => AlienType::ALIEN_HEAVY,
                    1 | 2 => AlienType::ALIEN_FAST,
                    _ => AlienType::ALIEN_BASIC,
                };

                let health = if alien_type == AlienType::ALIEN_HEAVY { 2 } else { 1 };

                self.aliens.push(Alien {
                    alien_id,
                    alien_type,
                    x: ALIEN_START_X + col as f32 * ALIEN_SPACING_X,
                    y: ALIEN_START_Y + row as f32 * ALIEN_SPACING_Y,
                    health,
                    alive: true,
                    row,
                    col,
                    timestamp_us: now_us(),
                });
                alien_id += 1;
            }
        }

        let total = self.aliens.len() as u16;
        self.game_state.current_wave = wave;
        self.game_state.aliens_total = total;
        self.game_state.aliens_remaining = total;
        self.game_state.alien_speed = 1.0 + (wave as f32 - 1.0) * 0.2;
        self.game_state.phase = GamePhase::PHASE_PLAYING;

        self.alien_direction = 1.0;
        self.alien_move_timer = 0.0;
    }

    fn update(&mut self, dt: f32) {
        if self.game_state.phase == GamePhase::PHASE_GAME_OVER {
            return;
        }

        self.kill_events.clear();

        self.update_player(dt);
        self.update_aliens(dt);
        self.update_bullets(dt);
        self.update_explosions();
        self.update_powerups(dt);
        self.check_collisions();
        self.alien_shoot();

        if self.game_state.aliens_remaining == 0 {
            self.game_state.phase = GamePhase::PHASE_WAVE_CLEAR;
        }

        let ts = now_us();
        self.player.timestamp_us = ts;
        self.game_state.timestamp_us = ts;
    }

    fn update_player(&mut self, dt: f32) {
        if !self.player.alive {
            return;
        }

        self.player.x += self.input_move as f32 * PLAYER_SPEED * dt;
        self.player.x = self.player.x.clamp(30.0, GAME_WIDTH - 30.0);

        self.fire_cooldown = (self.fire_cooldown - dt).max(0.0);

        if self.input_fire && self.fire_cooldown <= 0.0 {
            let cooldown = if self.player.power_up == PowerUpType::POWER_RAPID_FIRE {
                0.1
            } else {
                0.3
            };
            self.fire_cooldown = cooldown;

            if self.player.power_up == PowerUpType::POWER_TRIPLE_SHOT {
                for offset in [-20.0, 0.0, 20.0] {
                    self.spawn_bullet(self.player.x + offset, self.player.y - 20.0, true);
                }
            } else {
                self.spawn_bullet(self.player.x, self.player.y - 20.0, true);
            }
        }

        if self.player.power_up != PowerUpType::POWER_NONE {
            self.player.power_up_time -= dt;
            if self.player.power_up_time <= 0.0 {
                self.player.power_up = PowerUpType::POWER_NONE;
            }
        }
    }

    fn update_aliens(&mut self, dt: f32) {
        let alive_count = self.aliens.iter().filter(|a| a.alive).count();
        let speed_mult = 1.0 + (55 - alive_count.min(55)) as f32 * 0.02;
        let move_interval = 0.5 / (self.game_state.alien_speed * speed_mult);

        self.alien_move_timer += dt;

        if self.alien_move_timer >= move_interval {
            self.alien_move_timer = 0.0;

            let mut hit_edge = false;
            for alien in &self.aliens {
                if !alien.alive { continue; }
                if (self.alien_direction > 0.0 && alien.x >= GAME_WIDTH - 50.0) ||
                   (self.alien_direction < 0.0 && alien.x <= 50.0) {
                    hit_edge = true;
                    break;
                }
            }

            let ts = now_us();
            if hit_edge {
                for alien in &mut self.aliens {
                    if alien.alive {
                        alien.y += 20.0;
                        alien.timestamp_us = ts;
                    }
                }
                self.alien_direction *= -1.0;

                for alien in &self.aliens {
                    if alien.alive && alien.y >= PLAYER_Y - 30.0 {
                        self.game_state.phase = GamePhase::PHASE_GAME_OVER;
                        return;
                    }
                }
            } else {
                for alien in &mut self.aliens {
                    if alien.alive {
                        alien.x += self.alien_direction * 10.0;
                        alien.timestamp_us = ts;
                    }
                }
            }
        }
    }

    fn update_bullets(&mut self, dt: f32) {
        let ts = now_us();
        for bullet in &mut self.bullets {
            if !bullet.active { continue; }

            bullet.y += bullet.velocity_y * dt;
            bullet.timestamp_us = ts;

            if bullet.y < -10.0 || bullet.y > GAME_HEIGHT + 10.0 {
                bullet.active = false;
            }
        }

        self.bullets.retain(|b| b.active);
    }

    fn update_explosions(&mut self) {
        let ts = now_us();
        for exp in &mut self.explosions {
            if !exp.active { continue; }

            exp.frame = ((ts - exp.timestamp_us) / 100_000) as u8;
            if exp.frame >= 4 {
                exp.active = false;
            }
        }

        self.explosions.retain(|e| e.active);
    }

    fn update_powerups(&mut self, dt: f32) {
        let ts = now_us();
        for pu in &mut self.powerups {
            if !pu.active { continue; }

            pu.y += 50.0 * dt;
            pu.timestamp_us = ts;

            if pu.y > GAME_HEIGHT + 20.0 {
                pu.active = false;
            }
        }

        self.powerups.retain(|p| p.active);
    }

    fn check_collisions(&mut self) {
        let ts = now_us();

        // Player bullets vs aliens
        for bullet in &mut self.bullets {
            if !bullet.active || !bullet.from_player { continue; }

            for alien in &mut self.aliens {
                if !alien.alive { continue; }

                let dx = (bullet.x - alien.x).abs();
                let dy = (bullet.y - alien.y).abs();

                if dx < 24.0 && dy < 24.0 {
                    bullet.active = false;
                    alien.health -= 1;

                    if alien.health <= 0 {
                        alien.alive = false;
                        self.game_state.aliens_remaining -= 1;

                        let points = match alien.alien_type {
                            AlienType::ALIEN_BASIC => 10,
                            AlienType::ALIEN_FAST => 20,
                            AlienType::ALIEN_HEAVY => 30,
                            AlienType::ALIEN_BOSS => 100,
                        };
                        self.player.score += points as u32;

                        if self.player.score > self.game_state.high_score {
                            self.game_state.high_score = self.player.score;
                        }

                        self.explosions.push(Explosion {
                            explosion_id: self.next_explosion_id,
                            x: alien.x,
                            y: alien.y,
                            frame: 0,
                            active: true,
                            timestamp_us: ts,
                        });
                        self.next_explosion_id += 1;

                        self.kill_events.push(KillEvent {
                            event_id: self.next_event_id,
                            alien_id: alien.alien_id,
                            player_id: self.player.player_id,
                            alien_type: alien.alien_type,
                            points,
                            x: alien.x,
                            y: alien.y,
                            timestamp_us: ts,
                        });
                        self.next_event_id += 1;

                        // Maybe spawn powerup (10% chance)
                        let mut rng = rand::thread_rng();
                        if rng.gen_ratio(1, 10) {
                            let power_type = match rng.gen_range(0..3) {
                                0 => PowerUpType::POWER_RAPID_FIRE,
                                1 => PowerUpType::POWER_SHIELD,
                                _ => PowerUpType::POWER_TRIPLE_SHOT,
                            };
                            self.powerups.push(PowerUp {
                                powerup_id: self.next_powerup_id,
                                power_type,
                                x: alien.x,
                                y: alien.y,
                                active: true,
                                timestamp_us: ts,
                            });
                            self.next_powerup_id += 1;
                        }
                    }
                    break;
                }
            }
        }

        // Alien bullets vs player
        if self.player.alive {
            for bullet in &mut self.bullets {
                if !bullet.active || bullet.from_player { continue; }

                let dx = (bullet.x - self.player.x).abs();
                let dy = (bullet.y - self.player.y).abs();

                if dx < 20.0 && dy < 20.0 {
                    bullet.active = false;

                    if self.player.power_up == PowerUpType::POWER_SHIELD {
                        self.player.power_up = PowerUpType::POWER_NONE;
                        self.player.power_up_time = 0.0;
                    } else {
                        self.player.lives -= 1;

                        self.explosions.push(Explosion {
                            explosion_id: self.next_explosion_id,
                            x: self.player.x,
                            y: self.player.y,
                            frame: 0,
                            active: true,
                            timestamp_us: ts,
                        });
                        self.next_explosion_id += 1;

                        if self.player.lives == 0 {
                            self.player.alive = false;
                            self.game_state.phase = GamePhase::PHASE_GAME_OVER;
                        }
                    }
                    break;
                }
            }
        }

        // Player vs powerups
        for pu in &mut self.powerups {
            if !pu.active { continue; }

            let dx = (pu.x - self.player.x).abs();
            let dy = (pu.y - self.player.y).abs();

            if dx < 30.0 && dy < 30.0 {
                pu.active = false;
                self.player.power_up = pu.power_type;
                self.player.power_up_time = 10.0;
            }
        }
    }

    fn alien_shoot(&mut self) {
        let mut rng = rand::thread_rng();

        if !rng.gen_ratio(1, 60) { return; }

        let mut bottom_aliens: HashMap<u8, &Alien> = HashMap::new();
        for alien in &self.aliens {
            if !alien.alive { continue; }

            if let Some(existing) = bottom_aliens.get(&alien.col) {
                if alien.row > existing.row {
                    bottom_aliens.insert(alien.col, alien);
                }
            } else {
                bottom_aliens.insert(alien.col, alien);
            }
        }

        let shooters: Vec<_> = bottom_aliens.values().collect();
        if shooters.is_empty() { return; }

        let shooter = shooters[rng.gen_range(0..shooters.len())];
        self.spawn_bullet(shooter.x, shooter.y + 20.0, false);
    }

    fn spawn_bullet(&mut self, x: f32, y: f32, from_player: bool) {
        let velocity_y = if from_player { -BULLET_SPEED } else { ALIEN_BULLET_SPEED };

        self.bullets.push(Bullet {
            bullet_id: self.next_bullet_id,
            x,
            y,
            velocity_y,
            from_player,
            active: true,
            timestamp_us: now_us(),
        });
        self.next_bullet_id += 1;
    }

    #[allow(dead_code)]
    fn handle_input(&mut self, move_x: i8, fire: bool) {
        self.input_move = move_x;
        self.input_fire = fire;
    }

    #[allow(dead_code)]
    fn restart(&mut self) {
        self.player.lives = 3;
        self.player.score = 0;
        self.player.alive = true;
        self.player.x = GAME_WIDTH / 2.0;
        self.player.power_up = PowerUpType::POWER_NONE;
        self.spawn_wave(1);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("Space Invaders DDS - Starting...");

    // =========================================================================
    // Initialize HDDS DomainParticipant
    // =========================================================================
    let participant = Participant::builder("space_invaders")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    log::info!("[DDS] Participant created: space_invaders");

    // Create QoS for reliable delivery
    let qos = QoS::default();

    // Create DataWriters for each topic
    let player_writer = participant.topic::<Player>(topics::PLAYER)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let alien_writer = participant.topic::<Alien>(topics::ALIEN)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let bullet_writer = participant.topic::<Bullet>(topics::BULLET)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let explosion_writer = participant.topic::<Explosion>(topics::EXPLOSION)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let powerup_writer = participant.topic::<PowerUp>(topics::POWERUP)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let game_state_writer = participant.topic::<GameState>(topics::GAME_STATE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let kill_event_writer = participant.topic::<KillEvent>(topics::KILL_EVENT)?
        .writer()
        .qos(qos)
        .build()?;

    log::info!("[DDS] Writers created for all topics");

    // =========================================================================
    // Initialize game state
    // =========================================================================
    let mut game = GameSim::new();

    // =========================================================================
    // Game loop (60 FPS)
    // =========================================================================
    let dt = 1.0 / 60.0;
    let mut wave_clear_timer = 0.0;

    log::info!("[GAME] Starting game loop at 60 FPS");

    loop {
        // Handle wave clear delay
        if game.game_state.phase == GamePhase::PHASE_WAVE_CLEAR {
            wave_clear_timer += dt;
            if wave_clear_timer >= 2.0 {
                wave_clear_timer = 0.0;
                let next_wave = game.game_state.current_wave + 1;
                game.spawn_wave(next_wave);
            }
        } else {
            wave_clear_timer = 0.0;
        }

        game.update(dt);

        // =================================================================
        // Publish to DDS
        // =================================================================
        let _ = player_writer.write(&game.player);
        let _ = game_state_writer.write(&game.game_state);

        for alien in &game.aliens {
            let _ = alien_writer.write(alien);
        }

        for bullet in &game.bullets {
            let _ = bullet_writer.write(bullet);
        }

        for explosion in &game.explosions {
            let _ = explosion_writer.write(explosion);
        }

        for powerup in &game.powerups {
            let _ = powerup_writer.write(powerup);
        }

        for event in &game.kill_events {
            let _ = kill_event_writer.write(event);
        }

        tokio::time::sleep(tokio::time::Duration::from_secs_f32(dt)).await;
    }
}
