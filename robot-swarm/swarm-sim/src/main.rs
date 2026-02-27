//! Robot Swarm Simulator
//!
//! Simulates coordinated robot swarm behaviors:
//! - Flocking (boids algorithm)
//! - Formation patterns
//! - Follow leader
//! - Scatter/Gather
//! - Patrol waypoints

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::f32::consts::PI;
use swarm_types::swarm::*;

const TICK_RATE_HZ: u64 = 30;
const NUM_ROBOTS: usize = 12;

// Boids parameters
const SEPARATION_DIST: f32 = 25.0;
const ALIGNMENT_DIST: f32 = 50.0;
const COHESION_DIST: f32 = 80.0;
const SEPARATION_WEIGHT: f32 = 1.5;
const ALIGNMENT_WEIGHT: f32 = 1.0;
const COHESION_WEIGHT: f32 = 1.0;
const OBSTACLE_WEIGHT: f32 = 2.0;

// Arena and robot constants
const ARENA_WIDTH: f32 = 500.0;
const ARENA_HEIGHT: f32 = 500.0;
const ROBOT_RADIUS: f32 = 8.0;
const SENSOR_RANGE: f32 = 100.0;
const MAX_SPEED: f32 = 60.0;
const MAX_ROTATION: f32 = 3.0;

/// Topic names
mod topics {
    pub const ROBOT: &str = "swarm/robot";
    pub const SWARM_STATE: &str = "swarm/state";
    pub const OBSTACLE: &str = "swarm/obstacle";
    pub const WAYPOINT: &str = "swarm/waypoint";
}

/// Get current timestamp in microseconds
fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

struct SimRobot {
    robot: Robot,
    target_x: f32,
    target_y: f32,
    formation_offset_x: f32,
    formation_offset_y: f32,
}

impl SimRobot {
    fn new(id: u32, name: &str, color: u32, x: f32, y: f32) -> Self {
        Self {
            robot: Robot {
                robot_id: id,
                name: name.to_string(),
                color,
                x,
                y,
                heading: rand::thread_rng().gen_range(0.0..2.0 * PI),
                vx: 0.0,
                vy: 0.0,
                omega: 0.0,
                state: RobotState::IDLE,
                battery: rand::thread_rng().gen_range(70..100),
                front_dist: SENSOR_RANGE,
                left_dist: SENSOR_RANGE,
                right_dist: SENSOR_RANGE,
                is_leader: false,
                group_id: 0,
                timestamp_us: 0,
            },
            target_x: x,
            target_y: y,
            formation_offset_x: 0.0,
            formation_offset_y: 0.0,
        }
    }

    fn update_sensors(&mut self, robots: &[SimRobot], obstacles: &[Obstacle]) {
        // Simple distance sensors (front, left, right)
        let front_dir = (self.robot.heading.cos(), self.robot.heading.sin());
        let left_dir = (
            (self.robot.heading + PI / 4.0).cos(),
            (self.robot.heading + PI / 4.0).sin(),
        );
        let right_dir = (
            (self.robot.heading - PI / 4.0).cos(),
            (self.robot.heading - PI / 4.0).sin(),
        );

        self.robot.front_dist = SENSOR_RANGE;
        self.robot.left_dist = SENSOR_RANGE;
        self.robot.right_dist = SENSOR_RANGE;

        // Check other robots
        for other in robots {
            if other.robot.robot_id == self.robot.robot_id {
                continue;
            }
            let dx = other.robot.x - self.robot.x;
            let dy = other.robot.y - self.robot.y;
            let dist = (dx * dx + dy * dy).sqrt() - ROBOT_RADIUS * 2.0;

            if dist < SENSOR_RANGE && dist > 0.0 {
                let norm = (dx / (dist + ROBOT_RADIUS * 2.0), dy / (dist + ROBOT_RADIUS * 2.0));
                let front_dot = front_dir.0 * norm.0 + front_dir.1 * norm.1;
                let left_dot = left_dir.0 * norm.0 + left_dir.1 * norm.1;
                let right_dot = right_dir.0 * norm.0 + right_dir.1 * norm.1;

                if front_dot > 0.7 && dist < self.robot.front_dist {
                    self.robot.front_dist = dist;
                }
                if left_dot > 0.7 && dist < self.robot.left_dist {
                    self.robot.left_dist = dist;
                }
                if right_dot > 0.7 && dist < self.robot.right_dist {
                    self.robot.right_dist = dist;
                }
            }
        }

        // Check obstacles
        for obs in obstacles {
            let dx = obs.x - self.robot.x;
            let dy = obs.y - self.robot.y;
            let dist = (dx * dx + dy * dy).sqrt() - obs.radius - ROBOT_RADIUS;

            if dist < SENSOR_RANGE && dist > 0.0 {
                let norm = (dx / (dist + obs.radius + ROBOT_RADIUS), dy / (dist + obs.radius + ROBOT_RADIUS));
                let front_dot = front_dir.0 * norm.0 + front_dir.1 * norm.1;
                let left_dot = left_dir.0 * norm.0 + left_dir.1 * norm.1;
                let right_dot = right_dir.0 * norm.0 + right_dir.1 * norm.1;

                if front_dot > 0.7 && dist < self.robot.front_dist {
                    self.robot.front_dist = dist;
                }
                if left_dot > 0.7 && dist < self.robot.left_dist {
                    self.robot.left_dist = dist;
                }
                if right_dot > 0.7 && dist < self.robot.right_dist {
                    self.robot.right_dist = dist;
                }
            }
        }

        // Check arena walls
        let wall_front = check_wall_distance(self.robot.x, self.robot.y, front_dir);
        let wall_left = check_wall_distance(self.robot.x, self.robot.y, left_dir);
        let wall_right = check_wall_distance(self.robot.x, self.robot.y, right_dir);

        self.robot.front_dist = self.robot.front_dist.min(wall_front);
        self.robot.left_dist = self.robot.left_dist.min(wall_left);
        self.robot.right_dist = self.robot.right_dist.min(wall_right);
    }
}

fn check_wall_distance(x: f32, y: f32, dir: (f32, f32)) -> f32 {
    let mut min_dist = SENSOR_RANGE;

    // Check all 4 walls
    if dir.0 > 0.0 {
        let dist = (ARENA_WIDTH - x) / dir.0;
        if dist > 0.0 && dist < min_dist {
            min_dist = dist;
        }
    } else if dir.0 < 0.0 {
        let dist = -x / dir.0;
        if dist > 0.0 && dist < min_dist {
            min_dist = dist;
        }
    }

    if dir.1 > 0.0 {
        let dist = (ARENA_HEIGHT - y) / dir.1;
        if dist > 0.0 && dist < min_dist {
            min_dist = dist;
        }
    } else if dir.1 < 0.0 {
        let dist = -y / dir.1;
        if dist > 0.0 && dist < min_dist {
            min_dist = dist;
        }
    }

    min_dist
}

fn calculate_boids_velocity(
    robot: &SimRobot,
    robots: &[SimRobot],
    obstacles: &[Obstacle],
) -> (f32, f32) {
    let mut sep_x = 0.0;
    let mut sep_y = 0.0;
    let mut sep_count = 0;

    let mut align_vx = 0.0;
    let mut align_vy = 0.0;
    let mut align_count = 0;

    let mut coh_x = 0.0;
    let mut coh_y = 0.0;
    let mut coh_count = 0;

    for other in robots {
        if other.robot.robot_id == robot.robot.robot_id {
            continue;
        }
        if other.robot.group_id != robot.robot.group_id {
            continue;
        }

        let dx = other.robot.x - robot.robot.x;
        let dy = other.robot.y - robot.robot.y;
        let dist = (dx * dx + dy * dy).sqrt();

        // Separation
        if dist < SEPARATION_DIST && dist > 0.0 {
            sep_x -= dx / dist;
            sep_y -= dy / dist;
            sep_count += 1;
        }

        // Alignment
        if dist < ALIGNMENT_DIST {
            align_vx += other.robot.vx;
            align_vy += other.robot.vy;
            align_count += 1;
        }

        // Cohesion
        if dist < COHESION_DIST {
            coh_x += other.robot.x;
            coh_y += other.robot.y;
            coh_count += 1;
        }
    }

    let mut vx = 0.0;
    let mut vy = 0.0;

    // Apply separation
    if sep_count > 0 {
        vx += sep_x * SEPARATION_WEIGHT;
        vy += sep_y * SEPARATION_WEIGHT;
    }

    // Apply alignment
    if align_count > 0 {
        align_vx /= align_count as f32;
        align_vy /= align_count as f32;
        vx += align_vx * ALIGNMENT_WEIGHT;
        vy += align_vy * ALIGNMENT_WEIGHT;
    }

    // Apply cohesion
    if coh_count > 0 {
        coh_x /= coh_count as f32;
        coh_y /= coh_count as f32;
        let dx = coh_x - robot.robot.x;
        let dy = coh_y - robot.robot.y;
        vx += dx * 0.01 * COHESION_WEIGHT;
        vy += dy * 0.01 * COHESION_WEIGHT;
    }

    // Obstacle avoidance
    for obs in obstacles {
        let dx = robot.robot.x - obs.x;
        let dy = robot.robot.y - obs.y;
        let dist = (dx * dx + dy * dy).sqrt();
        let avoid_dist = obs.radius + SEPARATION_DIST;

        if dist < avoid_dist && dist > 0.0 {
            vx += dx / dist * OBSTACLE_WEIGHT * (avoid_dist - dist) / avoid_dist;
            vy += dy / dist * OBSTACLE_WEIGHT * (avoid_dist - dist) / avoid_dist;
        }
    }

    // Wall avoidance
    let margin = 30.0;
    if robot.robot.x < margin {
        vx += (margin - robot.robot.x) * 0.1;
    }
    if robot.robot.x > ARENA_WIDTH - margin {
        vx -= (robot.robot.x - (ARENA_WIDTH - margin)) * 0.1;
    }
    if robot.robot.y < margin {
        vy += (margin - robot.robot.y) * 0.1;
    }
    if robot.robot.y > ARENA_HEIGHT - margin {
        vy -= (robot.robot.y - (ARENA_HEIGHT - margin)) * 0.1;
    }

    (vx, vy)
}

fn calculate_formation_offset(index: usize, total: usize, formation: FormationType, spacing: f32) -> (f32, f32) {
    match formation {
        FormationType::LINE => {
            let offset = (index as f32 - (total - 1) as f32 / 2.0) * spacing;
            (offset, 0.0)
        }
        FormationType::CIRCLE => {
            let angle = 2.0 * PI * index as f32 / total as f32;
            let radius = spacing * total as f32 / (2.0 * PI);
            (angle.cos() * radius, angle.sin() * radius)
        }
        FormationType::WEDGE => {
            if index == 0 {
                (0.0, spacing)
            } else {
                let row = ((index as f32).sqrt()) as usize;
                let pos_in_row = index - row * row;
                let x_offset = (pos_in_row as f32 - row as f32 / 2.0) * spacing;
                let y_offset = -(row as f32) * spacing * 0.8;
                (x_offset, y_offset)
            }
        }
        FormationType::GRID => {
            let cols = (total as f32).sqrt().ceil() as usize;
            let row = index / cols;
            let col = index % cols;
            let x_offset = (col as f32 - (cols - 1) as f32 / 2.0) * spacing;
            let y_offset = (row as f32 - ((total - 1) / cols) as f32 / 2.0) * spacing;
            (x_offset, y_offset)
        }
        FormationType::DIAMOND => {
            if index == 0 {
                (0.0, spacing)
            } else if index == 1 {
                (0.0, -spacing)
            } else {
                let side = (index - 2) / 2;
                let sign = if (index - 2) % 2 == 0 { 1.0 } else { -1.0 };
                (sign * spacing * (1.0 + side as f32 * 0.5), 0.0)
            }
        }
        _ => (0.0, 0.0),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("==========================================");
    log::info!("  ROBOT SWARM - Simulator");
    log::info!("  HDDS Swarm Robotics Demo");
    log::info!("==========================================");

    let participant = Participant::builder("swarm_sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    let robot_writer = participant.create_writer::<Robot>(topics::ROBOT, qos.clone())?;
    let swarm_state_writer = participant.create_writer::<SwarmState>(topics::SWARM_STATE, qos.clone())?;
    let obstacle_writer = participant.create_writer::<Obstacle>(topics::OBSTACLE, qos.clone())?;
    let waypoint_writer = participant.create_writer::<Waypoint>(topics::WAYPOINT, qos)?;

    log::info!("DDS participant ready");

    // Initialize robots
    let mut rng = rand::thread_rng();
    let robot_names = [
        "ALPHA", "BRAVO", "CHARLIE", "DELTA", "ECHO", "FOXTROT",
        "GOLF", "HOTEL", "INDIA", "JULIET", "KILO", "LIMA",
    ];
    let robot_colors: [u32; 12] = [
        0xFF6B6B, 0x4ECDC4, 0x45B7D1, 0xF7DC6F, 0xBB8FCE, 0x82E0AA,
        0xF8B500, 0xE74C3C, 0x3498DB, 0x1ABC9C, 0x9B59B6, 0xE67E22,
    ];

    let mut robots: Vec<SimRobot> = (0..NUM_ROBOTS)
        .map(|i| {
            let x = rng.gen_range(100.0..ARENA_WIDTH - 100.0);
            let y = rng.gen_range(100.0..ARENA_HEIGHT - 100.0);
            let mut robot = SimRobot::new(i as u32, robot_names[i], robot_colors[i], x, y);
            if i == 0 {
                robot.robot.is_leader = true;
            }
            robot
        })
        .collect();

    // Initialize obstacles
    let obstacles: Vec<Obstacle> = vec![
        Obstacle {
            obstacle_id: 0,
            x: 150.0,
            y: 150.0,
            radius: 30.0,
            obstacle_type: 0,
        },
        Obstacle {
            obstacle_id: 1,
            x: 350.0,
            y: 350.0,
            radius: 40.0,
            obstacle_type: 0,
        },
        Obstacle {
            obstacle_id: 2,
            x: 250.0,
            y: 400.0,
            radius: 25.0,
            obstacle_type: 0,
        },
        Obstacle {
            obstacle_id: 3,
            x: 400.0,
            y: 150.0,
            radius: 35.0,
            obstacle_type: 0,
        },
    ];

    // Initialize waypoints for patrol
    let waypoints: Vec<Waypoint> = vec![
        Waypoint { waypoint_id: 0, x: 100.0, y: 100.0, radius: 20.0, seq_num: 0 },
        Waypoint { waypoint_id: 1, x: 400.0, y: 100.0, radius: 20.0, seq_num: 1 },
        Waypoint { waypoint_id: 2, x: 400.0, y: 400.0, radius: 20.0, seq_num: 2 },
        Waypoint { waypoint_id: 3, x: 100.0, y: 400.0, radius: 20.0, seq_num: 3 },
        Waypoint { waypoint_id: 4, x: 250.0, y: 250.0, radius: 20.0, seq_num: 4 },
    ];

    // Publish static data
    for obs in &obstacles {
        obstacle_writer.write(obs)?;
    }
    for wp in &waypoints {
        waypoint_writer.write(wp)?;
    }

    // Swarm state
    let mut current_mode = SwarmMode::FLOCK;
    let mut current_formation = FormationType::CIRCLE;
    let mut target_x = ARENA_WIDTH / 2.0;
    let mut target_y = ARENA_HEIGHT / 2.0;
    let mut mode_timer = 0.0;
    let mut current_waypoint = 0usize;

    let dt = 1.0 / TICK_RATE_HZ as f32;
    let tick_duration = std::time::Duration::from_millis(1000 / TICK_RATE_HZ);

    log::info!("Simulation running with {} robots", NUM_ROBOTS);

    loop {
        let tick_start = std::time::Instant::now();
        let timestamp = timestamp_us();

        // Auto-cycle through modes for demo
        mode_timer += dt;
        if mode_timer > 15.0 {
            mode_timer = 0.0;
            current_mode = match current_mode {
                SwarmMode::FLOCK => {
                    log::info!("Switching to FORMATION mode");
                    current_formation = FormationType::CIRCLE;
                    SwarmMode::FORMATION
                }
                SwarmMode::FORMATION => {
                    current_formation = match current_formation {
                        FormationType::LINE => FormationType::CIRCLE,
                        FormationType::CIRCLE => FormationType::WEDGE,
                        FormationType::WEDGE => FormationType::GRID,
                        FormationType::GRID => FormationType::DIAMOND,
                        _ => FormationType::LINE,
                    };
                    if current_formation == FormationType::LINE {
                        log::info!("Switching to FOLLOW_LEADER mode");
                        SwarmMode::FOLLOW_LEADER
                    } else {
                        log::info!("Formation: {:?}", current_formation);
                        SwarmMode::FORMATION
                    }
                }
                SwarmMode::FOLLOW_LEADER => {
                    log::info!("Switching to GATHER mode");
                    target_x = rng.gen_range(100.0..ARENA_WIDTH - 100.0);
                    target_y = rng.gen_range(100.0..ARENA_HEIGHT - 100.0);
                    SwarmMode::GATHER
                }
                SwarmMode::GATHER => {
                    log::info!("Switching to SCATTER mode");
                    SwarmMode::SCATTER
                }
                SwarmMode::SCATTER => {
                    log::info!("Switching to PATROL mode");
                    current_waypoint = 0;
                    SwarmMode::PATROL
                }
                SwarmMode::PATROL => {
                    log::info!("Switching to FLOCK mode");
                    SwarmMode::FLOCK
                }
                _ => SwarmMode::FLOCK,
            };
        }

        // Calculate formation offsets
        let spacing = 35.0;
        for (i, robot) in robots.iter_mut().enumerate() {
            let (ox, oy) = calculate_formation_offset(i, NUM_ROBOTS, current_formation, spacing);
            robot.formation_offset_x = ox;
            robot.formation_offset_y = oy;
        }

        // Get leader position
        let leader_x = robots[0].robot.x;
        let leader_y = robots[0].robot.y;

        // Update patrol target
        if current_mode == SwarmMode::PATROL {
            let wp = &waypoints[current_waypoint];
            let dx = wp.x - leader_x;
            let dy = wp.y - leader_y;
            if (dx * dx + dy * dy).sqrt() < wp.radius {
                current_waypoint = (current_waypoint + 1) % waypoints.len();
            }
            target_x = waypoints[current_waypoint].x;
            target_y = waypoints[current_waypoint].y;
        }

        // Clone positions for boids calculation
        let robot_snapshot: Vec<SimRobot> = robots
            .iter()
            .map(|r| SimRobot {
                robot: r.robot.clone(),
                target_x: r.target_x,
                target_y: r.target_y,
                formation_offset_x: r.formation_offset_x,
                formation_offset_y: r.formation_offset_y,
            })
            .collect();

        // Update each robot
        for (i, robot) in robots.iter_mut().enumerate() {
            robot.robot.timestamp_us = timestamp;

            // Update sensors
            robot.update_sensors(&robot_snapshot, &obstacles);

            // Calculate velocity based on mode
            let (mut vx, mut vy) = match current_mode {
                SwarmMode::FLOCK => {
                    calculate_boids_velocity(robot, &robot_snapshot, &obstacles)
                }
                SwarmMode::FORMATION => {
                    let tx = target_x + robot.formation_offset_x;
                    let ty = target_y + robot.formation_offset_y;
                    let dx = tx - robot.robot.x;
                    let dy = ty - robot.robot.y;
                    let (bx, by) = calculate_boids_velocity(robot, &robot_snapshot, &obstacles);
                    (dx * 0.05 + bx * 0.5, dy * 0.05 + by * 0.5)
                }
                SwarmMode::FOLLOW_LEADER => {
                    if i == 0 {
                        // Leader wanders
                        let angle = robot.robot.heading + rng.gen_range(-0.1..0.1);
                        (angle.cos() * MAX_SPEED * 0.5, angle.sin() * MAX_SPEED * 0.5)
                    } else {
                        // Followers follow leader with offset
                        let tx = leader_x + robot.formation_offset_x;
                        let ty = leader_y + robot.formation_offset_y;
                        let dx = tx - robot.robot.x;
                        let dy = ty - robot.robot.y;
                        let (bx, by) = calculate_boids_velocity(robot, &robot_snapshot, &obstacles);
                        (dx * 0.08 + bx * 0.3, dy * 0.08 + by * 0.3)
                    }
                }
                SwarmMode::SCATTER => {
                    // Move away from centroid
                    let mut cx = 0.0;
                    let mut cy = 0.0;
                    for r in &robot_snapshot {
                        cx += r.robot.x;
                        cy += r.robot.y;
                    }
                    cx /= NUM_ROBOTS as f32;
                    cy /= NUM_ROBOTS as f32;
                    let dx = robot.robot.x - cx;
                    let dy = robot.robot.y - cy;
                    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                    let (bx, by) = calculate_boids_velocity(robot, &robot_snapshot, &obstacles);
                    (dx / dist * 10.0 + bx, dy / dist * 10.0 + by)
                }
                SwarmMode::GATHER => {
                    let dx = target_x - robot.robot.x;
                    let dy = target_y - robot.robot.y;
                    let (bx, by) = calculate_boids_velocity(robot, &robot_snapshot, &obstacles);
                    (dx * 0.03 + bx * 0.3, dy * 0.03 + by * 0.3)
                }
                SwarmMode::PATROL => {
                    let dx = target_x - robot.robot.x;
                    let dy = target_y - robot.robot.y;
                    let (bx, by) = calculate_boids_velocity(robot, &robot_snapshot, &obstacles);
                    (dx * 0.05 + bx * 0.5, dy * 0.05 + by * 0.5)
                }
                _ => (0.0, 0.0),
            };

            // Limit speed
            let speed = (vx * vx + vy * vy).sqrt();
            if speed > MAX_SPEED {
                vx = vx / speed * MAX_SPEED;
                vy = vy / speed * MAX_SPEED;
            }

            // Smooth velocity change
            robot.robot.vx = robot.robot.vx * 0.8 + vx * 0.2;
            robot.robot.vy = robot.robot.vy * 0.8 + vy * 0.2;

            // Update position
            robot.robot.x += robot.robot.vx * dt;
            robot.robot.y += robot.robot.vy * dt;

            // Clamp to arena
            robot.robot.x = robot.robot.x.clamp(ROBOT_RADIUS, ARENA_WIDTH - ROBOT_RADIUS);
            robot.robot.y = robot.robot.y.clamp(ROBOT_RADIUS, ARENA_HEIGHT - ROBOT_RADIUS);

            // Update heading
            if robot.robot.vx.abs() > 0.1 || robot.robot.vy.abs() > 0.1 {
                let target_heading = robot.robot.vy.atan2(robot.robot.vx);
                let mut delta = target_heading - robot.robot.heading;
                while delta > PI {
                    delta -= 2.0 * PI;
                }
                while delta < -PI {
                    delta += 2.0 * PI;
                }
                robot.robot.omega = delta.clamp(-MAX_ROTATION, MAX_ROTATION);
                robot.robot.heading += robot.robot.omega * dt;
            }

            // Update state
            let speed = (robot.robot.vx * robot.robot.vx + robot.robot.vy * robot.robot.vy).sqrt();
            robot.robot.state = if speed > 1.0 {
                if robot.robot.front_dist < 20.0 {
                    RobotState::AVOIDING
                } else {
                    RobotState::MOVING
                }
            } else {
                RobotState::IDLE
            };

            // Simulate battery drain
            if rng.gen_ratio(1, 1000) && robot.robot.battery > 20 {
                robot.robot.battery -= 1;
            }

            // Publish robot state via DDS
            robot_writer.write(&robot.robot)?;
        }

        // Calculate and publish swarm state
        let mut centroid_x = 0.0;
        let mut centroid_y = 0.0;
        let mut total_battery = 0u32;
        for robot in &robots {
            centroid_x += robot.robot.x;
            centroid_y += robot.robot.y;
            total_battery += robot.robot.battery as u32;
        }
        centroid_x /= NUM_ROBOTS as f32;
        centroid_y /= NUM_ROBOTS as f32;

        let mut spread = 0.0;
        for robot in &robots {
            let dx = robot.robot.x - centroid_x;
            let dy = robot.robot.y - centroid_y;
            spread += (dx * dx + dy * dy).sqrt();
        }
        spread /= NUM_ROBOTS as f32;

        let swarm_state = SwarmState {
            robot_count: NUM_ROBOTS as u8,
            mode: current_mode,
            formation: current_formation,
            centroid_x,
            centroid_y,
            spread,
            avg_battery: (total_battery / NUM_ROBOTS as u32) as u8,
            timestamp_us: timestamp,
        };
        swarm_state_writer.write(&swarm_state)?;

        // Sleep to maintain tick rate
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        }
    }
}
