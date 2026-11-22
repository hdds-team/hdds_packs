//! Drone Racing - Race Server
//!
//! Manages race state, drone physics, and gate detection.

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use log::*;
use racing_types::racing::*;
use rand::Rng;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::time::{Duration, Instant};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

const TICK_RATE: u64 = 60;
const TICK_MS: u64 = 1000 / TICK_RATE;
const TOTAL_LAPS: u8 = 3;

// Physics constants
const TURN_RATE: f32 = 3.0;
const ACCELERATION: f32 = 40.0;
const LIFT: f32 = 12.0;
const GRAVITY: f32 = 9.81;
const DRAG: f32 = 0.02;
const MAX_SPEED: f32 = 60.0;

mod topics {
    pub const DRONE: &str = "Drone";
    pub const GATE: &str = "Gate";
    pub const GATE_PASS: &str = "GatePass";
    pub const RACE_STATE: &str = "RaceState";
    pub const LEADERBOARD: &str = "LeaderboardEntry";
    pub const INPUT: &str = "DroneInput";
}

/// Get current timestamp in microseconds
fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Drone colors (neon style)
const DRONE_COLORS: &[u32] = &[
    0xFF0066, // Pink
    0x00FF66, // Green
    0x6600FF, // Purple
    0xFFFF00, // Yellow
    0x00FFFF, // Cyan
    0xFF6600, // Orange
];

/// Generate race track gates in a circuit
fn generate_track() -> Vec<Gate> {
    let mut gates = Vec::new();

    // Oval-ish track with varying heights
    let gate_positions: &[(f32, f32, f32, f32)] = &[
        // x, y (height), z, rotation
        (0.0, 10.0, 0.0, 0.0),           // Start/Finish
        (80.0, 15.0, 50.0, 0.3),
        (150.0, 25.0, 120.0, 0.5),
        (180.0, 30.0, 200.0, PI * 0.3),
        (150.0, 35.0, 280.0, PI * 0.5),
        (80.0, 30.0, 320.0, PI * 0.7),
        (0.0, 25.0, 350.0, PI),
        (-80.0, 20.0, 300.0, PI * 1.3),
        (-120.0, 15.0, 200.0, PI * 1.5),
        (-80.0, 10.0, 100.0, PI * 1.8),
    ];

    for (i, &(x, y, z, rot)) in gate_positions.iter().enumerate() {
        gates.push(Gate {
            gate_id: i as u8,
            x,
            y,
            z,
            rotation: rot,
            width: 20.0,
            height: 15.0,
            is_start_finish: i == 0,
        });
    }

    gates
}

/// Drone state for physics simulation
struct DronePhysics {
    drone: Drone,
    input_throttle: f32,
    input_pitch: f32,
    input_yaw: f32,
    input_roll: f32,
    lap_start_time: Instant,
    race_start_time: Option<Instant>,
}

impl DronePhysics {
    fn new(id: u32, name: &str, color: u32, start_x: f32) -> Self {
        DronePhysics {
            drone: Drone {
                drone_id: id,
                name: name.to_string(),
                color,
                x: start_x,
                y: 5.0,
                z: -20.0,
                vx: 0.0,
                vy: 0.0,
                vz: 0.0,
                pitch: 0.0,
                yaw: 0.0,
                roll: 0.0,
                state: DroneState::READY,
                throttle: 0.0,
                current_gate: 0,
                lap: 0,
                position: 0,
                best_lap_ms: 0,
                total_time_ms: 0,
                timestamp_us: timestamp_us(),
            },
            input_throttle: 0.0,
            input_pitch: 0.0,
            input_yaw: 0.0,
            input_roll: 0.0,
            lap_start_time: Instant::now(),
            race_start_time: None,
        }
    }

    fn update(&mut self, dt: f32, racing: bool) {
        if self.drone.state == DroneState::CRASHED || self.drone.state == DroneState::DRONE_FINISHED {
            return;
        }

        if !racing && self.drone.state == DroneState::READY {
            return;
        }

        // Apply input to rotation
        self.drone.pitch += self.input_pitch * TURN_RATE * dt;
        self.drone.yaw += self.input_yaw * TURN_RATE * dt;
        self.drone.roll += self.input_roll * TURN_RATE * 2.0 * dt;

        // Clamp rotations
        self.drone.pitch = self.drone.pitch.clamp(-PI / 3.0, PI / 3.0);
        self.drone.roll = self.drone.roll.clamp(-PI / 2.0, PI / 2.0);

        // Auto-level roll when no input
        if self.input_roll.abs() < 0.1 {
            self.drone.roll *= 0.95;
        }

        // Throttle affects lift
        self.drone.throttle = self.input_throttle.clamp(0.0, 1.0);

        // Calculate thrust direction based on drone orientation
        let thrust = self.drone.throttle * ACCELERATION;

        // Forward thrust (based on pitch and yaw)
        let forward_x = self.drone.yaw.sin() * self.drone.pitch.cos();
        let forward_z = self.drone.yaw.cos() * self.drone.pitch.cos();
        let forward_y = -self.drone.pitch.sin();

        // Apply thrust
        self.drone.vx += forward_x * thrust * dt;
        self.drone.vy += forward_y * thrust * dt;
        self.drone.vz += forward_z * thrust * dt;

        // Lift (counteracts gravity when throttle is applied)
        let lift = self.drone.throttle * LIFT;
        self.drone.vy += lift * dt;

        // Gravity
        self.drone.vy -= GRAVITY * dt;

        // Drag
        self.drone.vx *= 1.0 - DRAG;
        self.drone.vy *= 1.0 - DRAG;
        self.drone.vz *= 1.0 - DRAG;

        // Speed limit
        let speed = (self.drone.vx.powi(2) + self.drone.vy.powi(2) + self.drone.vz.powi(2)).sqrt();
        if speed > MAX_SPEED {
            let scale = MAX_SPEED / speed;
            self.drone.vx *= scale;
            self.drone.vy *= scale;
            self.drone.vz *= scale;
        }

        // Update position
        self.drone.x += self.drone.vx * dt;
        self.drone.y += self.drone.vy * dt;
        self.drone.z += self.drone.vz * dt;

        // Ground collision
        if self.drone.y < 1.0 {
            self.drone.y = 1.0;
            self.drone.vy = 0.0;

            // Crash if hitting ground too fast
            if self.drone.vy < -20.0 {
                self.drone.state = DroneState::CRASHED;
            }
        }

        // Ceiling
        if self.drone.y > 100.0 {
            self.drone.y = 100.0;
            self.drone.vy = 0.0;
        }

        // Update timestamp
        self.drone.timestamp_us = timestamp_us();

        // Update total time
        if let Some(start) = self.race_start_time {
            self.drone.total_time_ms = start.elapsed().as_millis() as u32;
        }
    }

    fn check_gate(&mut self, gates: &[Gate]) -> Option<GatePass> {
        if self.drone.state != DroneState::DRONE_RACING {
            return None;
        }

        let gate = &gates[self.drone.current_gate as usize];

        // Simple distance check to gate center
        let dx = self.drone.x - gate.x;
        let dy = self.drone.y - gate.y;
        let dz = self.drone.z - gate.z;
        let dist = (dx * dx + dy * dy + dz * dz).sqrt();

        if dist < gate.width {
            let now = Instant::now();
            let lap_time = now.duration_since(self.lap_start_time).as_millis() as u32;

            let pass = GatePass {
                drone_id: self.drone.drone_id,
                gate_id: self.drone.current_gate,
                lap: self.drone.lap,
                time_ms: lap_time,
                timestamp_us: timestamp_us(),
            };

            self.drone.current_gate += 1;

            // Check if completed lap
            if self.drone.current_gate >= gates.len() as u8 {
                self.drone.current_gate = 0;
                self.drone.lap += 1;

                // Update best lap
                if self.drone.best_lap_ms == 0 || lap_time < self.drone.best_lap_ms {
                    self.drone.best_lap_ms = lap_time;
                }

                self.lap_start_time = now;

                // Check if finished race
                if self.drone.lap >= TOTAL_LAPS {
                    self.drone.state = DroneState::DRONE_FINISHED;
                }
            }

            return Some(pass);
        }

        None
    }

    fn start_race(&mut self) {
        self.drone.state = DroneState::DRONE_RACING;
        self.lap_start_time = Instant::now();
        self.race_start_time = Some(Instant::now());
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  DRONE RACING - Race Server");
    info!("  HDDS 3D Racing Demo");
    info!("==========================================");

    let participant = Participant::builder("racing_server")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    let drone_writer = participant.topic::<Drone>(topics::DRONE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let gate_writer = participant.topic::<Gate>(topics::GATE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let pass_writer = participant.topic::<GatePass>(topics::GATE_PASS)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let state_writer = participant.topic::<RaceState>(topics::RACE_STATE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let leaderboard_writer = participant.topic::<LeaderboardEntry>(topics::LEADERBOARD)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let input_reader = participant.topic::<DroneInput>(topics::INPUT)?
        .reader()
        .qos(qos)
        .build()?;

    info!("DDS participant ready");

    // Generate track
    let gates = generate_track();
    info!("Track generated: {} gates, {} laps", gates.len(), TOTAL_LAPS);

    // Publish gates
    for gate in &gates {
        gate_writer.write(gate)?;
    }
    const STATIC_REFRESH_S: f32 = 2.0;
    let mut last_gate_publish = Instant::now();

    // Create AI drones
    let mut rng = rand::thread_rng();
    let mut drones: HashMap<u32, DronePhysics> = HashMap::new();

    let drone_names = ["PHOENIX", "VIPER", "STORM", "BLAZE", "NOVA", "SHADOW"];
    for (i, name) in drone_names.iter().enumerate() {
        let id = 1001 + i as u32;
        let color = DRONE_COLORS[i % DRONE_COLORS.len()];
        let start_x = (i as f32 - 2.5) * 8.0;
        drones.insert(id, DronePhysics::new(id, name, color, start_x));
        info!("Drone {} ready at position {}", name, i + 1);
    }

    let mut race_state = RaceState {
        phase: RacePhase::WAITING,
        countdown: 3,
        total_laps: TOTAL_LAPS,
        elapsed_ms: 0,
        drone_count: drones.len() as u8,
        timestamp_us: timestamp_us(),
    };

    // Auto-start countdown after 3 seconds
    let mut countdown_start: Option<Instant> = None;
    let mut race_start: Option<Instant> = None;
    let mut last_tick = Instant::now();
    let mut tick_count: u64 = 0;

    // Start countdown immediately for demo
    countdown_start = Some(Instant::now() + Duration::from_secs(2));
    race_state.phase = RacePhase::COUNTDOWN;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Static data, re-sent for readers that join later: a reader
        // only receives what is written after discovery matched it.
        if now.duration_since(last_gate_publish).as_secs_f32() > STATIC_REFRESH_S {
            for gate in &gates {
                gate_writer.write(gate)?;
            }
            last_gate_publish = now;
        }

        // Process inputs
        while let Ok(Some(input)) = input_reader.take() {
            if let Some(drone) = drones.get_mut(&input.drone_id) {
                drone.input_throttle = input.throttle;
                drone.input_pitch = input.pitch;
                drone.input_yaw = input.yaw;
                drone.input_roll = input.roll;
            }
        }

        // AI inputs
        for (id, drone) in &mut drones {
            if *id >= 1000 {
                // Simple AI: aim towards next gate
                let target_gate = &gates[drone.drone.current_gate as usize];
                let dx = target_gate.x - drone.drone.x;
                let dy = target_gate.y - drone.drone.y;
                let dz = target_gate.z - drone.drone.z;

                // Calculate desired yaw
                let target_yaw = dx.atan2(dz);
                let yaw_diff = (target_yaw - drone.drone.yaw).sin();

                // Calculate desired pitch
                let horiz_dist = (dx * dx + dz * dz).sqrt();
                let target_pitch = (-dy).atan2(horiz_dist);
                let pitch_diff = target_pitch - drone.drone.pitch;

                // Apply AI input with some randomness
                drone.input_yaw = (yaw_diff * 2.0).clamp(-1.0, 1.0) + rng.gen_range(-0.1..0.1);
                drone.input_pitch = (pitch_diff * 1.5).clamp(-1.0, 1.0) + rng.gen_range(-0.1..0.1);
                drone.input_throttle = 0.7 + rng.gen_range(0.0..0.3);
                drone.input_roll = rng.gen_range(-0.2..0.2);
            }
        }

        // Update race phase
        match race_state.phase {
            RacePhase::COUNTDOWN => {
                if let Some(start) = countdown_start {
                    if now >= start {
                        let elapsed = now.duration_since(start).as_secs();
                        if elapsed < 3 {
                            race_state.countdown = 3 - elapsed as u8;
                        } else {
                            // Start race!
                            race_state.phase = RacePhase::PHASE_RACING;
                            race_state.countdown = 0;
                            race_start = Some(now);

                            for drone in drones.values_mut() {
                                drone.start_race();
                            }
                            info!("RACE STARTED!");
                        }
                    }
                }
            }
            RacePhase::PHASE_RACING => {
                if let Some(start) = race_start {
                    race_state.elapsed_ms = start.elapsed().as_millis() as u32;
                }

                // Check if all drones finished
                if drones.values().all(|d| d.drone.state == DroneState::DRONE_FINISHED) {
                    race_state.phase = RacePhase::PHASE_FINISHED;
                    info!("RACE FINISHED!");
                }
            }
            _ => {}
        }

        // Update drones
        let racing = race_state.phase == RacePhase::PHASE_RACING;
        for drone in drones.values_mut() {
            drone.update(dt, racing);

            // Check gate passes
            if let Some(pass) = drone.check_gate(&gates) {
                info!("{} passed gate {} (lap {})", drone.drone.name, pass.gate_id, pass.lap);
                pass_writer.write(&pass)?;
            }
        }

        // Calculate positions
        let mut sorted: Vec<_> = drones.values()
            .map(|d| (d.drone.drone_id, d.drone.lap as i32 * 100 + d.drone.current_gate as i32))
            .collect();
        sorted.sort_by(|a, b| b.1.cmp(&a.1));

        for (pos, (drone_id, _)) in sorted.iter().enumerate() {
            if let Some(drone) = drones.get_mut(drone_id) {
                drone.drone.position = pos as u8 + 1;
            }
        }

        // Broadcast state
        for drone in drones.values() {
            drone_writer.write(&drone.drone)?;
        }

        // Broadcast leaderboard every 10 ticks
        if tick_count % 10 == 0 {
            let mut sorted: Vec<_> = drones.values().collect();
            sorted.sort_by_key(|d| d.drone.position);

            for drone in sorted {
                let entry = LeaderboardEntry {
                    position: drone.drone.position,
                    drone_id: drone.drone.drone_id,
                    drone_name: drone.drone.name.clone(),
                    lap: drone.drone.lap,
                    gate: drone.drone.current_gate,
                    total_time_ms: drone.drone.total_time_ms,
                    best_lap_ms: drone.drone.best_lap_ms,
                    timestamp_us: timestamp_us(),
                };
                leaderboard_writer.write(&entry)?;
            }
        }

        // Broadcast race state
        if tick_count % TICK_RATE == 0 {
            race_state.timestamp_us = timestamp_us();
            state_writer.write(&race_state)?;
        }

        tick_count += 1;

        let elapsed = last_tick.elapsed();
        if elapsed < Duration::from_millis(TICK_MS) {
            tokio::time::sleep(Duration::from_millis(TICK_MS) - elapsed).await;
        }
    }
}
