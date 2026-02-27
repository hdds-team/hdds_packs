//! LiDAR Mapping Simulator
//!
//! Simulates a robot with LiDAR sensor exploring an environment:
//! - Virtual maze environment with walls
//! - Raycasting-based LiDAR simulation
//! - Occupancy grid mapping
//! - Simple exploration behavior

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use lidar_types::lidar::*;
use rand::Rng;
use std::f32::consts::PI;
use std::time::UNIX_EPOCH;

// Simulation constants
const LIDAR_RANGE: f32 = 500.0;
const LIDAR_RAYS: usize = 360;
const MAP_CELLS_X: usize = 200;
const MAP_CELLS_Y: usize = 200;
const MAP_RESOLUTION: f32 = 5.0;
const WORLD_WIDTH: f32 = 1000.0;
const WORLD_HEIGHT: f32 = 1000.0;
const ROBOT_RADIUS: f32 = 20.0;

const TICK_RATE_HZ: u64 = 30;
const SCAN_RATE_HZ: u64 = 5; // LiDAR scans per second
const MAX_SPEED: f32 = 40.0; // cm/s
const MAX_ROTATION: f32 = 2.0; // rad/s

// Helper modules for constants
mod control_mode {
    use super::ControlMode;
    pub const MANUAL: ControlMode = ControlMode::MANUAL;
    pub const EXPLORE: ControlMode = ControlMode::EXPLORE;
    pub const WAYPOINT: ControlMode = ControlMode::WAYPOINT;
}

mod occupancy {
    pub const FREE: u8 = 0;
    pub const UNKNOWN: u8 = 127;
    pub const OCCUPIED: u8 = 255;
}

mod topics {
    pub const ROBOT_POSE: &str = "RobotPose";
    pub const LIDAR_BATCH: &str = "LidarBatch";
    pub const MAP_BATCH: &str = "MapBatch";
    pub const WALL: &str = "WallSegment";
}

/// Get current timestamp in microseconds
fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Wall segment in the environment
#[derive(Debug, Clone)]
struct Wall {
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
}

impl Wall {
    fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Ray-line intersection, returns distance or None
    fn intersect_ray(&self, ox: f32, oy: f32, dx: f32, dy: f32) -> Option<f32> {
        let x1 = self.x1;
        let y1 = self.y1;
        let x2 = self.x2;
        let y2 = self.y2;

        let x3 = ox;
        let y3 = oy;
        let x4 = ox + dx * LIDAR_RANGE;
        let y4 = oy + dy * LIDAR_RANGE;

        let denom = (x1 - x2) * (y3 - y4) - (y1 - y2) * (x3 - x4);
        if denom.abs() < 0.0001 {
            return None; // Parallel
        }

        let t = ((x1 - x3) * (y3 - y4) - (y1 - y3) * (x3 - x4)) / denom;
        let u = -((x1 - x2) * (y1 - y3) - (y1 - y2) * (x1 - x3)) / denom;

        if t >= 0.0 && t <= 1.0 && u >= 0.0 && u <= 1.0 {
            let ix = x1 + t * (x2 - x1);
            let iy = y1 + t * (y2 - y1);
            let dist = ((ix - ox).powi(2) + (iy - oy).powi(2)).sqrt();
            if dist <= LIDAR_RANGE {
                return Some(dist);
            }
        }
        None
    }
}

/// Robot state
struct Robot {
    x: f32,
    y: f32,
    heading: f32,
    vx: f32,
    vy: f32,
    omega: f32,
    mode: ControlMode,
    target_x: f32,
    target_y: f32,
}

impl Robot {
    fn new(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            heading: 0.0,
            vx: 0.0,
            vy: 0.0,
            omega: 0.0,
            mode: control_mode::EXPLORE,
            target_x: x,
            target_y: y,
        }
    }
}

/// Occupancy grid map
struct OccupancyMap {
    cells: Vec<Vec<u8>>,
    width: usize,
    height: usize,
}

impl OccupancyMap {
    fn new() -> Self {
        Self {
            cells: vec![vec![occupancy::UNKNOWN; MAP_CELLS_Y]; MAP_CELLS_X],
            width: MAP_CELLS_X,
            height: MAP_CELLS_Y,
        }
    }

    fn world_to_cell(&self, x: f32, y: f32) -> (usize, usize) {
        let cx = ((x / MAP_RESOLUTION) as usize).min(self.width - 1);
        let cy = ((y / MAP_RESOLUTION) as usize).min(self.height - 1);
        (cx, cy)
    }

    fn update_ray(&mut self, ox: f32, oy: f32, angle: f32, distance: f32) -> Vec<(u16, u16, u8)> {
        let mut updates = Vec::new();
        let dx = angle.cos();
        let dy = angle.sin();

        // Mark free cells along the ray
        let step = MAP_RESOLUTION / 2.0;
        let max_dist = if distance > 0.0 { distance } else { LIDAR_RANGE };

        let mut d = 0.0;
        while d < max_dist - step {
            let px = ox + dx * d;
            let py = oy + dy * d;

            if px >= 0.0 && px < WORLD_WIDTH && py >= 0.0 && py < WORLD_HEIGHT {
                let (cx, cy) = self.world_to_cell(px, py);
                if self.cells[cx][cy] != occupancy::FREE {
                    self.cells[cx][cy] = occupancy::FREE;
                    updates.push((cx as u16, cy as u16, occupancy::FREE));
                }
            }
            d += step;
        }

        // Mark occupied cell if hit detected
        if distance > 0.0 && distance < LIDAR_RANGE - 1.0 {
            let hx = ox + dx * distance;
            let hy = oy + dy * distance;

            if hx >= 0.0 && hx < WORLD_WIDTH && hy >= 0.0 && hy < WORLD_HEIGHT {
                let (cx, cy) = self.world_to_cell(hx, hy);
                if self.cells[cx][cy] != occupancy::OCCUPIED {
                    self.cells[cx][cy] = occupancy::OCCUPIED;
                    updates.push((cx as u16, cy as u16, occupancy::OCCUPIED));
                }
            }
        }

        updates
    }
}

/// Create maze-like environment
fn create_environment() -> Vec<Wall> {
    let mut walls = Vec::new();

    // Outer boundary
    walls.push(Wall::new(50.0, 50.0, 950.0, 50.0));   // Top
    walls.push(Wall::new(950.0, 50.0, 950.0, 950.0)); // Right
    walls.push(Wall::new(950.0, 950.0, 50.0, 950.0)); // Bottom
    walls.push(Wall::new(50.0, 950.0, 50.0, 50.0));   // Left

    // Internal walls - creates a maze-like structure
    // Horizontal walls
    walls.push(Wall::new(150.0, 200.0, 400.0, 200.0));
    walls.push(Wall::new(500.0, 200.0, 700.0, 200.0));
    walls.push(Wall::new(200.0, 350.0, 450.0, 350.0));
    walls.push(Wall::new(600.0, 350.0, 850.0, 350.0));
    walls.push(Wall::new(100.0, 500.0, 300.0, 500.0));
    walls.push(Wall::new(400.0, 500.0, 600.0, 500.0));
    walls.push(Wall::new(700.0, 500.0, 900.0, 500.0));
    walls.push(Wall::new(150.0, 650.0, 350.0, 650.0));
    walls.push(Wall::new(500.0, 650.0, 750.0, 650.0));
    walls.push(Wall::new(200.0, 800.0, 500.0, 800.0));
    walls.push(Wall::new(600.0, 800.0, 850.0, 800.0));

    // Vertical walls
    walls.push(Wall::new(300.0, 100.0, 300.0, 200.0));
    walls.push(Wall::new(600.0, 100.0, 600.0, 200.0));
    walls.push(Wall::new(200.0, 200.0, 200.0, 350.0));
    walls.push(Wall::new(500.0, 200.0, 500.0, 350.0));
    walls.push(Wall::new(800.0, 200.0, 800.0, 350.0));
    walls.push(Wall::new(350.0, 350.0, 350.0, 500.0));
    walls.push(Wall::new(650.0, 350.0, 650.0, 500.0));
    walls.push(Wall::new(150.0, 500.0, 150.0, 650.0));
    walls.push(Wall::new(450.0, 500.0, 450.0, 650.0));
    walls.push(Wall::new(750.0, 500.0, 750.0, 650.0));
    walls.push(Wall::new(250.0, 650.0, 250.0, 800.0));
    walls.push(Wall::new(550.0, 650.0, 550.0, 800.0));
    walls.push(Wall::new(850.0, 650.0, 850.0, 800.0));

    // Some obstacles (represented as small boxes via 4 walls)
    let obstacles = [
        (400.0, 400.0, 40.0),
        (700.0, 250.0, 30.0),
        (250.0, 700.0, 35.0),
        (800.0, 700.0, 25.0),
    ];

    for (ox, oy, size) in obstacles {
        walls.push(Wall::new(ox - size, oy - size, ox + size, oy - size));
        walls.push(Wall::new(ox + size, oy - size, ox + size, oy + size));
        walls.push(Wall::new(ox + size, oy + size, ox - size, oy + size));
        walls.push(Wall::new(ox - size, oy + size, ox - size, oy - size));
    }

    walls
}

/// Perform LiDAR scan
fn perform_scan(robot: &Robot, walls: &[Wall]) -> Vec<(f32, f32)> {
    let mut readings = Vec::with_capacity(LIDAR_RAYS);

    for i in 0..LIDAR_RAYS {
        let angle = robot.heading + (i as f32 * 2.0 * PI / LIDAR_RAYS as f32);
        let dx = angle.cos();
        let dy = angle.sin();

        // Find closest wall intersection
        let mut min_dist = LIDAR_RANGE;
        for wall in walls {
            if let Some(dist) = wall.intersect_ray(robot.x, robot.y, dx, dy) {
                if dist < min_dist {
                    min_dist = dist;
                }
            }
        }

        // If no hit, use 0 to indicate max range
        let distance = if min_dist < LIDAR_RANGE - 0.1 { min_dist } else { 0.0 };
        readings.push((angle, distance));
    }

    readings
}

/// Simple exploration behavior
fn explore_behavior(robot: &mut Robot, scan: &[(f32, f32)], rng: &mut impl Rng) {
    // Find closest obstacle in front
    let mut front_clear = true;
    let mut left_clear = true;
    let mut right_clear = true;

    for (angle, distance) in scan {
        if *distance > 0.0 && *distance < 80.0 {
            let rel_angle = (*angle - robot.heading).rem_euclid(2.0 * PI);

            if rel_angle < 0.5 || rel_angle > 5.8 {
                front_clear = false;
            } else if rel_angle < 1.5 {
                right_clear = false;
            } else if rel_angle > 4.8 {
                left_clear = false;
            }
        }
    }

    // Simple exploration: go forward if clear, otherwise turn
    let target_speed;
    let target_omega;

    if front_clear {
        target_speed = MAX_SPEED * 0.7;
        target_omega = rng.gen_range(-0.2..0.2);
    } else if left_clear {
        target_speed = MAX_SPEED * 0.3;
        target_omega = MAX_ROTATION * 0.5;
    } else if right_clear {
        target_speed = MAX_SPEED * 0.3;
        target_omega = -MAX_ROTATION * 0.5;
    } else {
        target_speed = -MAX_SPEED * 0.3;
        target_omega = MAX_ROTATION * 0.8 * if rng.gen_bool(0.5) { 1.0 } else { -1.0 };
    }

    // Smooth velocity changes
    robot.vx = robot.vx * 0.9 + target_speed * robot.heading.cos() * 0.1;
    robot.vy = robot.vy * 0.9 + target_speed * robot.heading.sin() * 0.1;
    robot.omega = robot.omega * 0.9 + target_omega * 0.1;
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("==========================================");
    log::info!("  LIDAR MAPPING - Simulator");
    log::info!("  HDDS SLAM Demo");
    log::info!("==========================================");

    let participant = Participant::builder("lidar_sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    let pose_writer = participant.create_writer::<RobotPose>(topics::ROBOT_POSE, qos.clone())?;
    let batch_writer = participant.create_writer::<LidarBatch>(topics::LIDAR_BATCH, qos.clone())?;
    let map_batch_writer = participant.create_writer::<MapBatch>(topics::MAP_BATCH, qos.clone())?;
    let wall_writer = participant.create_writer::<WallSegment>(topics::WALL, qos)?;

    log::info!("DDS participant ready");

    // Create environment
    let walls = create_environment();
    log::info!("Environment created with {} wall segments", walls.len());

    // Publish walls to viewer
    for (i, wall) in walls.iter().enumerate() {
        let ws = WallSegment {
            wall_id: i as u32,
            x1: wall.x1,
            y1: wall.y1,
            x2: wall.x2,
            y2: wall.y2,
            thickness: 5.0,
        };
        wall_writer.write(&ws)?;
    }

    // Initialize robot
    let mut robot = Robot::new(500.0, 500.0);
    let mut rng = rand::thread_rng();

    // Initialize occupancy map
    let mut map = OccupancyMap::new();

    let dt = 1.0 / TICK_RATE_HZ as f32;
    let scan_interval = TICK_RATE_HZ / SCAN_RATE_HZ;
    let mut tick_count: u64 = 0;
    let mut scan_id: u32 = 0;
    let mut map_batch_id: u32 = 0;

    let tick_duration = std::time::Duration::from_millis(1000 / TICK_RATE_HZ);

    log::info!("Simulation running - robot starting at ({}, {})", robot.x, robot.y);

    loop {
        let tick_start = std::time::Instant::now();
        let timestamp = timestamp_us();

        // Perform LiDAR scan at reduced rate
        if tick_count % scan_interval == 0 {
            let scan = perform_scan(&robot, &walls);

            // Update map and collect cell updates
            let mut all_updates = Vec::new();
            for (angle, distance) in &scan {
                let updates = map.update_ray(robot.x, robot.y, *angle, *distance);
                all_updates.extend(updates);
            }

            // Send map updates in batches
            if !all_updates.is_empty() {
                // Flatten cell updates into a Vec<u16>: [x0, y0, occ0, x1, y1, occ1, ...]
                let cells: Vec<u16> = all_updates.iter().flat_map(|(x, y, o)| [*x, *y, *o as u16]).collect();
                let map_batch = MapBatch {
                    batch_id: map_batch_id,
                    cells,
                    timestamp_us: timestamp,
                };
                // DDS write may fail with large data - ignore errors
                let _ = map_batch_writer.write(&map_batch);
                map_batch_id += 1;
            }

            // Send LiDAR data in batches (90 readings per batch)
            let batch_size = 90;
            let num_batches = (scan.len() + batch_size - 1) / batch_size;

            for batch_idx in 0..num_batches {
                let start = batch_idx * batch_size;
                let end = (start + batch_size).min(scan.len());
                // Flatten readings into a Vec<f32>: [angle0, dist0, angle1, dist1, ...]
                let readings: Vec<f32> = scan[start..end].iter().flat_map(|(a, d)| [*a, *d]).collect();

                let lidar_batch = LidarBatch {
                    scan_id,
                    robot_id: 0,
                    robot_x: robot.x,
                    robot_y: robot.y,
                    robot_heading: robot.heading,
                    batch_index: batch_idx as u8,
                    batch_count: num_batches as u8,
                    readings,
                    timestamp_us: timestamp,
                };
                // DDS write may fail with large data - ignore errors
                let _ = batch_writer.write(&lidar_batch);
            }

            scan_id += 1;

            // Update exploration behavior based on scan
            explore_behavior(&mut robot, &scan, &mut rng);
        }

        // Update robot physics
        robot.x += robot.vx * dt;
        robot.y += robot.vy * dt;
        robot.heading += robot.omega * dt;

        // Keep heading in [0, 2*PI]
        while robot.heading < 0.0 {
            robot.heading += 2.0 * PI;
        }
        while robot.heading >= 2.0 * PI {
            robot.heading -= 2.0 * PI;
        }

        // Clamp to world bounds
        robot.x = robot.x.clamp(ROBOT_RADIUS + 50.0, WORLD_WIDTH - ROBOT_RADIUS - 50.0);
        robot.y = robot.y.clamp(ROBOT_RADIUS + 50.0, WORLD_HEIGHT - ROBOT_RADIUS - 50.0);

        // Publish robot pose
        let pose = RobotPose {
            robot_id: 0,
            x: robot.x,
            y: robot.y,
            heading: robot.heading,
            vx: robot.vx,
            vy: robot.vy,
            omega: robot.omega,
            mode: robot.mode,
            timestamp_us: timestamp,
        };
        pose_writer.write(&pose)?;

        tick_count += 1;

        // Sleep to maintain tick rate
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}
