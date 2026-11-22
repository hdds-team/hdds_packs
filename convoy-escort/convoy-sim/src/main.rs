//! Convoy Simulator
//!
//! Simulates a VIP convoy with multiple vehicles:
//! - LEAD escort (Humvee)
//! - VIP vehicle (armored SUV)
//! - REAR escort (Humvee)
//!
//! Publishes:
//! - VehicleGPS at 10Hz
//! - VehicleState at 1Hz
//! - ConvoySummary at 1Hz

use convoy_types::convoy::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::f64::consts::PI;
use std::time::{Duration, Instant};
use tracing::{info, debug};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

/// Topic names
mod topics {
    pub const VEHICLE_GPS: &str = "convoy/vehicle/gps";
    pub const VEHICLE_STATE: &str = "convoy/vehicle/state";
    pub const CONVOY_SUMMARY: &str = "convoy/summary";
    pub const CONVOY_ALERT: &str = "convoy/alert";
}

/// Simulated vehicle state
struct Vehicle {
    id: String,
    role: VehicleRole,
    lat: f64,
    lon: f64,
    heading: f32,
    speed: f32,
    status: VehicleStatus,
    fuel: f32,
    /// Target offset behind lead vehicle (negative = behind)
    follow_offset_lon: f64,
    /// Target lateral offset from lead (for formation)
    follow_offset_lat: f64,
}

impl Vehicle {
    fn new(id: &str, role: VehicleRole, lat: f64, lon: f64, heading: f32) -> Self {
        Self {
            id: id.to_string(),
            role,
            lat,
            lon,
            heading,
            speed: 0.0,
            status: VehicleStatus::OPERATIONAL,
            fuel: 100.0,
            follow_offset_lon: 0.0,
            follow_offset_lat: 0.0,
        }
    }

    fn update(&mut self, dt: f32, target_speed: f32, lead_heading: f32, lead_lat: f64, lead_lon: f64) {
        // Accelerate/decelerate toward target speed
        let accel = 5.0; // km/h per second
        if self.speed < target_speed {
            self.speed = (self.speed + accel * dt).min(target_speed);
        } else if self.speed > target_speed {
            self.speed = (self.speed - accel * dt).max(target_speed);
        }

        // Formation keeping: steer toward lead heading + correct position
        let heading_error = lead_heading - self.heading;
        // Normalize to [-180, 180] (rem_euclid for proper modular arithmetic)
        let heading_error = (heading_error + 180.0).rem_euclid(360.0) - 180.0;
        // Smooth correction toward lead heading
        self.heading += heading_error * 0.3 * dt;

        // Position correction: nudge toward target formation position
        let target_lat = lead_lat + self.follow_offset_lat;
        let target_lon = lead_lon + self.follow_offset_lon;
        let lat_err = target_lat - self.lat;
        let lon_err = target_lon - self.lon;
        // Gentle position correction (prevents drifting apart)
        self.lat += lat_err * 0.5 * dt as f64;
        self.lon += lon_err * 0.5 * dt as f64;

        // Move vehicle based on heading and speed
        let speed_deg_per_sec = self.speed / 3600.0 / 111.0;
        let heading_rad = (self.heading as f64) * PI / 180.0;
        self.lat += speed_deg_per_sec as f64 * heading_rad.cos() * dt as f64;
        self.lon += speed_deg_per_sec as f64 * heading_rad.sin() * dt as f64
            / (self.lat * PI / 180.0).cos();

        // Normalize heading to [0, 360)
        self.heading = self.heading.rem_euclid(360.0);

        // Consume fuel
        self.fuel -= 0.001 * self.speed * dt;
        self.fuel = self.fuel.max(0.0);
    }

    fn to_gps(&self, timestamp_ms: u64) -> VehicleGPS {
        VehicleGPS {
            vehicle_id: self.id.clone(),
            role: self.role,
            latitude: self.lat,
            longitude: self.lon,
            altitude_m: 100.0,
            heading_deg: self.heading,
            speed_kmh: self.speed,
            course_deg: self.heading,
            timestamp_ms,
        }
    }

    fn to_state(&self) -> VehicleState {
        VehicleState {
            vehicle_id: self.id.clone(),
            status: self.status,
            fuel_percent: self.fuel,
            battery_volts: 12.6,
            passengers: match self.role {
                VehicleRole::VIP => 3,
                _ => 2,
            },
            lights_on: true,
            sirens_on: false,
            driver_name: format!("Driver_{}", self.id),
        }
    }
}

fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    info!("=== CONVOY ESCORT SIMULATOR ===");
    info!("Starting HDDS participant...");

    // Create DDS participant
    let participant = Participant::builder("convoy_sim")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;
    info!("[OK] DDS participant created");

    // Create QoS profile
    let qos = QoS::best_effort().keep_last(10);

    // Create writers
    let writer_gps = participant.topic::<VehicleGPS>(topics::VEHICLE_GPS)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let writer_state = participant.topic::<VehicleState>(topics::VEHICLE_STATE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let writer_summary = participant.topic::<ConvoySummary>(topics::CONVOY_SUMMARY)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let writer_alert = participant.topic::<ConvoyAlert>(topics::CONVOY_ALERT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    info!("[OK] DDS writers created");

    // Initialize convoy vehicles
    // Starting position: Paris area (48.8566 N, 2.3522 E)
    // Heading North (0 degrees) so latitude increases -> Z decreases in scene
    // This makes vehicles move ALONG the road (Z axis) in the viewer
    let base_lat = 48.8566;
    let base_lon = 2.3522;
    let heading = 0.0_f32; // North

    // Formation offsets (relative to lead):
    // Heading North: "behind" = smaller latitude, "right" = larger longitude
    let spacing = 0.0003; // ~33 meters between vehicles

    let mut lead = Vehicle::new("LEAD-1", VehicleRole::LEAD, base_lat, base_lon, heading);
    lead.follow_offset_lat = 0.0;
    lead.follow_offset_lon = 0.0;

    let mut vip = Vehicle::new("VIP-EAGLE", VehicleRole::VIP, base_lat - spacing, base_lon, heading);
    vip.follow_offset_lat = -spacing;
    vip.follow_offset_lon = 0.0;

    let mut escort = Vehicle::new("ESCORT-2", VehicleRole::ESCORT, base_lat - spacing, base_lon + spacing * 0.5, heading);
    escort.follow_offset_lat = -spacing;
    escort.follow_offset_lon = spacing * 0.5;

    let mut rear = Vehicle::new("REAR-3", VehicleRole::REAR, base_lat - spacing * 2.0, base_lon, heading);
    rear.follow_offset_lat = -spacing * 2.0;
    rear.follow_offset_lon = 0.0;

    let mut vehicles = vec![lead, vip, escort, rear];

    info!("[OK] Convoy initialized with {} vehicles", vehicles.len());
    info!("    LEAD-1    : Lead escort");
    info!("    VIP-EAGLE : VIP vehicle");
    info!("    ESCORT-2  : Side escort");
    info!("    REAR-3    : Rear guard");
    info!("");
    info!("Publishing GPS @ 10Hz, Status @ 1Hz");
    info!("Press Ctrl+C to stop");

    // Simulation parameters
    let target_speed = 60.0; // km/h
    let dt = 0.1; // 100ms tick

    let mut tick = 0u64;
    let start = Instant::now();

    loop {
        let loop_start = Instant::now();
        tick += 1;

        // Update lead vehicle first (independent movement with gentle drift)
        {
            let mut rng = rand::thread_rng();
            // Mean-revert heading toward North (0 deg) so convoy stays on the road
            let target_heading = 0.0_f32;
            let drift = ((target_heading - vehicles[0].heading + 180.0).rem_euclid(360.0)) - 180.0;
            vehicles[0].heading += drift * 0.1 * dt + rng.gen_range(-0.15..0.15);
            vehicles[0].heading = vehicles[0].heading.rem_euclid(360.0);
            let h = vehicles[0].heading;
            let la = vehicles[0].lat;
            let lo = vehicles[0].lon;
            vehicles[0].update(dt, target_speed, h, la, lo);
        }

        // Update followers: they track the lead vehicle
        let lead_heading = vehicles[0].heading;
        let lead_lat = vehicles[0].lat;
        let lead_lon = vehicles[0].lon;
        for i in 1..vehicles.len() {
            let mut rng = rand::thread_rng();
            vehicles[i].heading += rng.gen_range(-0.1..0.1);
            vehicles[i].update(dt, target_speed, lead_heading, lead_lat, lead_lon);
        }

        // Publish GPS at 10Hz (every tick)
        let ts = timestamp_ms();
        for v in &vehicles {
            let gps = v.to_gps(ts);
            if let Err(e) = writer_gps.write(&gps) {
                tracing::error!("Failed to publish GPS: {}", e);
            }
        }

        // Publish state at 1Hz (every 10 ticks)
        if tick % 10 == 0 {
            for v in &vehicles {
                let state = v.to_state();
                if let Err(e) = writer_state.write(&state) {
                    tracing::error!("Failed to publish state: {}", e);
                }
            }

            // Publish convoy summary
            let center_lat: f64 = vehicles.iter().map(|v| v.lat).sum::<f64>() / vehicles.len() as f64;
            let center_lon: f64 = vehicles.iter().map(|v| v.lon).sum::<f64>() / vehicles.len() as f64;
            let avg_speed: f32 = vehicles.iter().map(|v| v.speed).sum::<f32>() / vehicles.len() as f32;

            let summary = ConvoySummary {
                convoy_id: "CONVOY-ALPHA".to_string(),
                convoy_name: "VIP Escort Paris-Berlin".to_string(),
                total_vehicles: vehicles.len() as u8,
                operational_count: vehicles.iter().filter(|v| v.status == VehicleStatus::OPERATIONAL).count() as u8,
                current_threat: ThreatLevel::GREEN,
                current_formation: FormationType::COLUMN,
                progress_percent: (start.elapsed().as_secs_f32() / 3600.0 * 100.0).min(100.0),
                center_lat,
                center_lon,
                avg_speed_kmh: avg_speed,
                eta_ms: ts + 3600_000, // +1h
            };

            if let Err(e) = writer_summary.write(&summary) {
                tracing::error!("Failed to publish summary: {}", e);
            }

            // Log progress
            debug!(
                "Convoy @ ({:.4}, {:.4}) | {:.1} km/h | {} vehicles operational",
                center_lat, center_lon, avg_speed, summary.operational_count
            );
        }

        // Simulate an alert every 30 seconds
        if tick % 300 == 150 {
            let mut rng = rand::thread_rng();
            let alert = ConvoyAlert {
                alert_id: tick as u32,
                alert_type: match rng.gen_range(0..3) {
                    0 => AlertType::ROADBLOCK,
                    1 => AlertType::CIVILIAN_HAZARD,
                    _ => AlertType::ALL_CLEAR,
                },
                threat_level: ThreatLevel::YELLOW,
                latitude: vehicles[0].lat + rng.gen_range(-0.01..0.01),
                longitude: vehicles[0].lon + rng.gen_range(0.01..0.05),
                radius_m: 100.0,
                description: "Potential hazard ahead".to_string(),
                reported_by: "LEAD-1".to_string(),
                timestamp_ms: ts,
            };

            if let Err(e) = writer_alert.write(&alert) {
                tracing::error!("Failed to publish alert: {}", e);
            }
            info!("⚠️  Alert: {:?} at ({:.4}, {:.4})", alert.alert_type, alert.latitude, alert.longitude);
        }

        // Sleep to maintain 10Hz
        let elapsed = loop_start.elapsed();
        if elapsed < Duration::from_millis(100) {
            tokio::time::sleep(Duration::from_millis(100) - elapsed).await;
        }
    }
}
