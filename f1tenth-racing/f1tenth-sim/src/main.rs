//! F1Tenth Autonomous Racing - Race Simulator
//!
//! 4 autonomous cars on a closed circuit using bicycle model physics.
//! Publishes car telemetry, race state, lap events via DDS (HDDS).

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use f1tenth_types::f1tenth::*;
use log::*;
use rand::Rng;
use std::f32::consts::PI;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// --- Sim constants ---

const TICK_RATE: u64 = 60;
const TICK_MS: u64 = 1000 / TICK_RATE;
const TOTAL_LAPS: u8 = 5;
const DOMAIN_ID: u32 = 99; // demos

// --- Physics constants (F1Tenth scale: ~1/10th real car) ---

const WHEELBASE: f32 = 0.33;       // meters (F1Tenth standard)
const MAX_SPEED: f32 = 8.0;        // m/s (~29 km/h)
const MAX_STEER: f32 = 0.42;       // radians (~24 degrees)
const ACCEL: f32 = 4.0;            // m/s^2
const BRAKE_DECEL: f32 = 8.0;      // m/s^2
const DRAG: f32 = 0.3;             // drag coefficient
const CORNERING_GRIP: f32 = 6.0;   // lateral grip limit (m/s^2)

// --- DDS topic names ---

mod topics {
    pub const CAR: &str = "f1tenth/car";
    pub const TRACK_POINT: &str = "f1tenth/track_point";
    pub const RACE_STATE: &str = "f1tenth/race_state";
    pub const LAP_EVENT: &str = "f1tenth/lap_event";
    pub const SECTOR_TIME: &str = "f1tenth/sector_time";
}

fn timestamp_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}

// --- Track definition ---

struct TrackDef {
    points: Vec<(f32, f32)>,  // (x, z) centerline
    widths: Vec<f32>,
    cumulative_dist: Vec<f32>,
    total_length: f32,
}

impl TrackDef {
    fn generate_circuit() -> Self {
        // Oval-ish circuit with chicane, inspired by F1Tenth tracks
        // Coordinates in meters, centered around origin
        let control_points: &[(f32, f32)] = &[
            // Start/finish straight
            (0.0, 0.0),
            (3.0, 0.0),
            (6.0, 0.0),
            // Turn 1 (wide right)
            (8.0, 0.5),
            (9.5, 2.0),
            (10.0, 4.0),
            // Back straight
            (10.0, 6.0),
            (9.5, 8.0),
            // Chicane
            (8.0, 9.5),
            (6.5, 10.0),
            (5.0, 10.5),
            (3.5, 10.0),
            // Turn 3 (hairpin)
            (2.0, 8.5),
            (1.5, 7.0),
            (2.0, 5.5),
            // Short straight
            (3.0, 4.5),
            (3.5, 3.5),
            // Turn 4 (back to start)
            (3.0, 2.0),
            (2.0, 1.0),
            (0.5, 0.2),
        ];

        // Subdivide for smoother track
        let mut points = Vec::new();
        for i in 0..control_points.len() {
            let (x0, z0) = control_points[i];
            let (x1, z1) = control_points[(i + 1) % control_points.len()];
            let steps = 5;
            for s in 0..steps {
                let t = s as f32 / steps as f32;
                points.push((x0 + (x1 - x0) * t, z0 + (z1 - z0) * t));
            }
        }

        let n = points.len();
        let widths = vec![0.6; n]; // 1.2m total width

        // Compute cumulative distance
        let mut cumulative_dist = Vec::with_capacity(n);
        cumulative_dist.push(0.0);
        for i in 1..n {
            let dx = points[i].0 - points[i - 1].0;
            let dz = points[i].1 - points[i - 1].1;
            let d = (dx * dx + dz * dz).sqrt();
            cumulative_dist.push(cumulative_dist[i - 1] + d);
        }
        // Close the loop
        let dx = points[0].0 - points[n - 1].0;
        let dz = points[0].1 - points[n - 1].1;
        let total_length = cumulative_dist[n - 1] + (dx * dx + dz * dz).sqrt();

        TrackDef { points, widths, cumulative_dist, total_length }
    }

    fn sector_for_progress(&self, progress: f32) -> u8 {
        if progress < 0.333 { 0 }
        else if progress < 0.666 { 1 }
        else { 2 }
    }

    fn closest_point_index(&self, x: f32, z: f32) -> usize {
        let mut best_i = 0;
        let mut best_dist = f32::MAX;
        for (i, &(px, pz)) in self.points.iter().enumerate() {
            let d = (x - px) * (x - px) + (z - pz) * (z - pz);
            if d < best_dist {
                best_dist = d;
                best_i = i;
            }
        }
        best_i
    }

    fn progress_at(&self, idx: usize) -> f32 {
        self.cumulative_dist[idx] / self.total_length
    }

    fn point_at_progress(&self, progress: f32) -> (f32, f32) {
        let target_dist = progress * self.total_length;
        let n = self.points.len();
        for i in 1..n {
            if self.cumulative_dist[i] >= target_dist {
                let seg_start = self.cumulative_dist[i - 1];
                let seg_len = self.cumulative_dist[i] - seg_start;
                let t = if seg_len > 0.0 { (target_dist - seg_start) / seg_len } else { 0.0 };
                let (x0, z0) = self.points[i - 1];
                let (x1, z1) = self.points[i];
                return (x0 + (x1 - x0) * t, z0 + (z1 - z0) * t);
            }
        }
        self.points[n - 1]
    }

    fn heading_at(&self, idx: usize) -> f32 {
        let n = self.points.len();
        let next = (idx + 1) % n;
        let dx = self.points[next].0 - self.points[idx].0;
        let dz = self.points[next].1 - self.points[idx].1;
        dz.atan2(dx)
    }
}

// --- Car physics (bicycle model) ---

struct CarPhysics {
    car: Car,
    // AI parameters (per-car variation)
    skill: f32,           // 0.8 - 1.0, affects speed/precision
    aggression: f32,      // 0.0 - 1.0, affects braking point
    // Timing
    lap_start: Instant,
    sector_start: Instant,
    race_start: Option<Instant>,
    prev_sector: u8,
    // Track progress tracking
    prev_closest_idx: usize,
    laps_completed_raw: f32,  // continuous progress counter
}

impl CarPhysics {
    fn new(id: u32, name: &str, color: u32, track: &TrackDef, grid_pos: usize) -> Self {
        // Stagger start positions along the start straight
        let start_progress = -(grid_pos as f32) * 0.015; // slight offset behind start
        let (sx, sz) = track.point_at_progress(start_progress.rem_euclid(1.0));
        let heading = track.heading_at(0);

        CarPhysics {
            car: Car {
                car_id: id,
                name: name.to_string(),
                color,
                x: sx,
                y: 0.0,
                z: sz,
                vx: 0.0,
                vz: 0.0,
                speed: 0.0,
                heading,
                steering_angle: 0.0,
                state: CarState::READY,
                lap: 0,
                total_laps: TOTAL_LAPS,
                sector: 0,
                track_progress: 0.0,
                best_lap_ms: 0,
                last_lap_ms: 0,
                total_time_ms: 0,
                position: (grid_pos + 1) as u8,
                timestamp_us: timestamp_us(),
            },
            skill: 0.0,
            aggression: 0.0,
            lap_start: Instant::now(),
            sector_start: Instant::now(),
            race_start: None,
            prev_sector: 0,
            prev_closest_idx: 0,
            laps_completed_raw: 0.0,
        }
    }

    fn start_race(&mut self) {
        self.car.state = CarState::RACING;
        let now = Instant::now();
        self.lap_start = now;
        self.sector_start = now;
        self.race_start = Some(now);
    }

    fn update(&mut self, dt: f32, track: &TrackDef, racing: bool) {
        if self.car.state != CarState::RACING || !racing {
            self.car.timestamp_us = timestamp_us();
            return;
        }

        // --- AI: compute desired steering and throttle ---
        let (throttle, steer) = self.compute_ai(track);

        // --- Bicycle model update ---
        // Steering
        self.car.steering_angle = steer.clamp(-MAX_STEER, MAX_STEER);

        // Speed
        if throttle > 0.0 {
            self.car.speed += throttle * ACCEL * dt;
        } else {
            self.car.speed += throttle * BRAKE_DECEL * dt;
        }
        // Drag
        self.car.speed -= self.car.speed * DRAG * dt;
        self.car.speed = self.car.speed.clamp(0.0, MAX_SPEED * self.skill);

        // Bicycle model kinematics
        if self.car.steering_angle.abs() > 0.001 {
            let turn_radius = WHEELBASE / self.car.steering_angle.tan();
            let angular_vel = self.car.speed / turn_radius;
            self.car.heading += angular_vel * dt;
        }

        // Normalize heading
        self.car.heading = self.car.heading.rem_euclid(2.0 * PI);

        // Velocity from heading + speed
        self.car.vx = self.car.heading.cos() * self.car.speed;
        self.car.vz = self.car.heading.sin() * self.car.speed;

        // Position update
        self.car.x += self.car.vx * dt;
        self.car.z += self.car.vz * dt;

        // --- Track progress ---
        let idx = track.closest_point_index(self.car.x, self.car.z);
        let new_progress = track.progress_at(idx);

        // Detect lap crossing (progress wraps from ~1.0 to ~0.0)
        let old_progress = self.car.track_progress;
        if old_progress > 0.8 && new_progress < 0.2 {
            self.laps_completed_raw += 1.0;
        } else if old_progress < 0.2 && new_progress > 0.8 {
            self.laps_completed_raw -= 1.0; // going backwards
        }

        self.car.track_progress = new_progress;
        self.prev_closest_idx = idx;

        // Sector detection
        let sector = track.sector_for_progress(new_progress);
        self.car.sector = sector;

        // Timing
        if let Some(start) = self.race_start {
            self.car.total_time_ms = start.elapsed().as_millis() as u32;
        }

        self.car.timestamp_us = timestamp_us();
    }

    fn compute_ai(&self, track: &TrackDef) -> (f32, f32) {

        // Look ahead on the track
        let lookahead_dist = 0.8 + self.car.speed * 0.15; // farther at higher speed
        let lookahead_progress = (self.car.track_progress
            + lookahead_dist / track.total_length).rem_euclid(1.0);
        let (target_x, target_z) = track.point_at_progress(lookahead_progress);

        // Angle to target
        let dx = target_x - self.car.x;
        let dz = target_z - self.car.z;
        let target_heading = dz.atan2(dx);

        // Heading error
        let mut heading_err = target_heading - self.car.heading;
        // Normalize to [-PI, PI]
        while heading_err > PI { heading_err -= 2.0 * PI; }
        while heading_err < -PI { heading_err += 2.0 * PI; }

        // Steering proportional to heading error
        let steer = (heading_err * 2.5 * self.skill).clamp(-MAX_STEER, MAX_STEER);

        // Throttle: slow down in curves
        let curvature = self.estimate_curvature(track);
        let max_corner_speed = (CORNERING_GRIP / curvature.max(0.1)).sqrt();
        let target_speed = max_corner_speed.min(MAX_SPEED) * self.skill;

        let throttle = if self.car.speed < target_speed * 0.9 {
            0.8 + self.aggression * 0.2
        } else if self.car.speed > target_speed * 1.1 {
            -0.5 - self.aggression * 0.3
        } else {
            0.3
        };

        (throttle, steer)
    }

    fn estimate_curvature(&self, track: &TrackDef) -> f32 {
        let n = track.points.len();
        let idx = self.prev_closest_idx;
        // Sample 3 points ahead
        let i0 = idx;
        let i1 = (idx + 5) % n;
        let i2 = (idx + 10) % n;

        let (x0, z0) = track.points[i0];
        let (x1, z1) = track.points[i1];
        let (x2, z2) = track.points[i2];

        // Menger curvature from 3 points
        let area = ((x1 - x0) * (z2 - z0) - (x2 - x0) * (z1 - z0)).abs();
        let d01 = ((x1 - x0).powi(2) + (z1 - z0).powi(2)).sqrt();
        let d12 = ((x2 - x1).powi(2) + (z2 - z1).powi(2)).sqrt();
        let d02 = ((x2 - x0).powi(2) + (z2 - z0).powi(2)).sqrt();
        let denom = d01 * d12 * d02;

        if denom > 0.001 {
            4.0 * area / denom
        } else {
            0.0
        }
    }

    fn check_lap(&mut self, _track: &TrackDef) -> Option<LapEvent> {
        let old_lap = self.car.lap;
        let expected_lap = self.laps_completed_raw.floor() as u8;

        if expected_lap > old_lap && old_lap < TOTAL_LAPS {
            let now = Instant::now();
            let lap_time_ms = now.duration_since(self.lap_start).as_millis() as u32;

            self.car.lap = expected_lap;
            self.car.last_lap_ms = lap_time_ms;

            if self.car.best_lap_ms == 0 || lap_time_ms < self.car.best_lap_ms {
                self.car.best_lap_ms = lap_time_ms;
            }

            self.lap_start = now;
            self.sector_start = now;

            if self.car.lap >= TOTAL_LAPS {
                self.car.state = CarState::FINISHED;
            }

            return Some(LapEvent {
                car_id: self.car.car_id,
                lap: expected_lap,
                lap_time_ms,
                best_lap_ms: self.car.best_lap_ms,
                timestamp_us: timestamp_us(),
            });
        }
        None
    }

    fn check_sector(&mut self) -> Option<SectorTime> {
        let sector = self.car.sector;
        if sector != self.prev_sector {
            let now = Instant::now();
            let sector_time_ms = now.duration_since(self.sector_start).as_millis() as u32;
            self.sector_start = now;
            let prev = self.prev_sector;
            self.prev_sector = sector;

            return Some(SectorTime {
                car_id: self.car.car_id,
                sector: prev,
                sector_time_ms,
                timestamp_us: timestamp_us(),
            });
        }
        None
    }
}

// --- Car definitions ---

const CAR_DEFS: &[(u32, &str, u32, f32, f32)] = &[
    //  id,   name,       color,     skill, aggression
    (1, "APEX",     0xFF2200, 0.97, 0.85),  // Red - fast, aggressive
    (2, "PHANTOM",  0x00CCFF, 0.95, 0.70),  // Cyan - consistent
    (3, "VORTEX",   0xFFCC00, 0.92, 0.90),  // Yellow - wild
    (4, "SHADOW",   0xAA00FF, 0.93, 0.60),  // Purple - cautious
];

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  F1TENTH AUTONOMOUS RACING");
    info!("  HDDS Real-Time DDS Demo");
    info!("==========================================");

    let participant = Participant::builder("f1tenth_sim")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    let car_writer = participant.create_writer::<Car>(topics::CAR, qos.clone())?;
    let track_writer = participant.create_writer::<TrackPoint>(topics::TRACK_POINT, qos.clone())?;
    let state_writer = participant.create_writer::<RaceState>(topics::RACE_STATE, qos.clone())?;
    let lap_writer = participant.create_writer::<LapEvent>(topics::LAP_EVENT, qos.clone())?;
    let sector_writer = participant.create_writer::<SectorTime>(topics::SECTOR_TIME, qos.clone())?;

    info!("DDS participant ready (domain {})", DOMAIN_ID);

    // Generate track
    let track = TrackDef::generate_circuit();
    info!("Track: {} points, {:.1}m total length", track.points.len(), track.total_length);

    // Publish track points
    for (i, ((x, z), w)) in track.points.iter().zip(track.widths.iter()).enumerate() {
        track_writer.write(&TrackPoint {
            index: i as u16,
            x: *x,
            z: *z,
            width: *w,
        })?;
    }
    info!("Track published ({} points)", track.points.len());

    // Create cars
    let mut rng = rand::thread_rng();
    let mut cars: Vec<CarPhysics> = Vec::new();

    for (i, &(id, name, color, skill, aggression)) in CAR_DEFS.iter().enumerate() {
        let mut car = CarPhysics::new(id, name, color, &track, i);
        // Add slight randomness to skill
        car.skill = skill + rng.gen_range(-0.02..0.02);
        car.aggression = aggression + rng.gen_range(-0.05..0.05);
        info!("Car {} [#{:06X}] ready at P{} (skill={:.2})", name, color, i + 1, car.skill);
        cars.push(car);
    }

    let mut race_state = RaceState {
        phase: RacePhase::WAITING,
        countdown: 3,
        total_laps: TOTAL_LAPS,
        elapsed_ms: 0,
        car_count: cars.len() as u8,
        timestamp_us: timestamp_us(),
    };

    // Auto-start countdown after 2 seconds
    let countdown_start = Instant::now() + Duration::from_secs(2);
    let mut race_start: Option<Instant> = None;
    let mut last_tick = Instant::now();
    let mut tick_count: u64 = 0;

    race_state.phase = RacePhase::COUNTDOWN;
    info!("Countdown starting...");

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32().min(0.05); // cap dt
        last_tick = now;

        // --- Race phase management ---
        match race_state.phase {
            RacePhase::COUNTDOWN => {
                if now >= countdown_start {
                    let elapsed_secs = now.duration_since(countdown_start).as_secs();
                    if elapsed_secs < 3 {
                        race_state.countdown = 3 - elapsed_secs as u8;
                    } else {
                        race_state.phase = RacePhase::RACING;
                        race_state.countdown = 0;
                        race_start = Some(now);
                        for car in &mut cars {
                            car.start_race();
                        }
                        info!("LIGHTS OUT AND AWAY WE GO!");
                    }
                }
            }
            RacePhase::RACING => {
                if let Some(start) = race_start {
                    race_state.elapsed_ms = start.elapsed().as_millis() as u32;
                }
                if cars.iter().all(|c| c.car.state == CarState::FINISHED) {
                    race_state.phase = RacePhase::FINISHED;
                    info!("CHECKERED FLAG!");
                    for car in &cars {
                        info!("  P{} {} - {:.1}s (best lap: {:.3}s)",
                            car.car.position, car.car.name,
                            car.car.total_time_ms as f32 / 1000.0,
                            car.car.best_lap_ms as f32 / 1000.0);
                    }
                }
            }
            RacePhase::FINISHED => {
                // Keep publishing for 10 seconds then exit
                if race_start.map(|s| s.elapsed().as_secs() > 30).unwrap_or(false) {
                    info!("Race complete. Shutting down.");
                    break;
                }
            }
            _ => {}
        }

        let racing = race_state.phase == RacePhase::RACING;

        // --- Update physics ---
        for car in &mut cars {
            car.update(dt, &track, racing);

            // Check lap completion
            if let Some(lap_event) = car.check_lap(&track) {
                info!("{} completed lap {} ({:.3}s{})",
                    car.car.name, lap_event.lap,
                    lap_event.lap_time_ms as f32 / 1000.0,
                    if lap_event.lap_time_ms == car.car.best_lap_ms { " BEST" } else { "" });
                lap_writer.write(&lap_event)?;
            }

            // Check sector
            if let Some(sector_time) = car.check_sector() {
                sector_writer.write(&sector_time)?;
            }
        }

        // --- Calculate positions ---
        // Finished cars ranked by total_time (lower = better),
        // then racing cars by laps + progress (higher = better)
        let mut rankings: Vec<(usize, u64)> = cars.iter().enumerate()
            .map(|(i, c)| {
                let score = if c.car.state == CarState::FINISHED {
                    // Finished cars get highest priority, ordered by finish time
                    // Lower time = higher score (invert so sort descending works)
                    u64::MAX - c.car.total_time_ms as u64
                } else {
                    // Racing cars scored by progress (laps + fractional)
                    ((c.laps_completed_raw + c.car.track_progress) * 1_000_000.0) as u64
                };
                (i, score)
            })
            .collect();
        rankings.sort_by(|a, b| b.1.cmp(&a.1));

        for (pos, &(car_idx, _)) in rankings.iter().enumerate() {
            cars[car_idx].car.position = (pos + 1) as u8;
        }

        // --- Publish car states (every tick = 60Hz) ---
        for car in &cars {
            car_writer.write(&car.car)?;
        }

        // --- Publish race state (every 30 ticks = 2Hz) ---
        if tick_count % 30 == 0 {
            race_state.timestamp_us = timestamp_us();
            state_writer.write(&race_state)?;
        }

        tick_count += 1;

        // Sleep to maintain tick rate
        let elapsed = last_tick.elapsed();
        if elapsed < Duration::from_millis(TICK_MS) {
            tokio::time::sleep(Duration::from_millis(TICK_MS) - elapsed).await;
        }
    }

    Ok(())
}
