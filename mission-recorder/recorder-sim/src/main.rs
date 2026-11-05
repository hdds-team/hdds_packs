// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! Mission Recorder - Simulation
//!
//! Generates realistic mission telemetry for 4 units on patrol routes,
//! publishing MissionEvent and MissionStatus over HDDS DDS topics.
//!
//! - Position updates at 2 Hz per unit
//! - Contact events at random intervals
//! - Status updates at 0.5 Hz
//! - 5-minute mission with phase progression

use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use recorder_types::{topics, MissionEvent, MissionStatus};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Total mission duration in seconds.
const MISSION_DURATION_S: f64 = 300.0;

/// Position update interval (2 Hz).
const POSITION_INTERVAL: Duration = Duration::from_millis(500);

/// Status update interval (0.5 Hz).
const STATUS_INTERVAL: Duration = Duration::from_secs(2);

// =============================================================================
// UNIT STATE
// =============================================================================

struct Unit {
    name: &'static str,
    /// Current latitude (decimal degrees).
    lat: f64,
    /// Current longitude (decimal degrees).
    lon: f64,
    /// Altitude in meters.
    alt: f64,
    /// Heading in degrees (0-360).
    heading: f32,
    /// Speed in knots.
    speed: f32,
    /// Waypoint index for patrol route.
    wp_idx: usize,
    /// Patrol waypoints: (lat, lon).
    waypoints: Vec<(f64, f64)>,
}

impl Unit {
    fn new(name: &'static str, waypoints: Vec<(f64, f64)>) -> Self {
        let (lat, lon) = waypoints[0];
        Self {
            name,
            lat,
            lon,
            alt: 0.0,
            heading: 0.0,
            speed: 12.0,
            wp_idx: 0,
            waypoints,
        }
    }

    /// Advance the unit toward its next waypoint.
    fn tick(&mut self, dt_s: f64, rng: &mut impl Rng) {
        if self.waypoints.is_empty() {
            return;
        }

        let target = self.waypoints[(self.wp_idx + 1) % self.waypoints.len()];
        let dlat = target.0 - self.lat;
        let dlon = target.1 - self.lon;
        let dist = (dlat * dlat + dlon * dlon).sqrt();

        // Heading toward waypoint (simplified flat-earth)
        self.heading = (dlon.atan2(dlat).to_degrees() as f32 + 360.0) % 360.0;

        // Move toward waypoint (~1 knot = ~0.0000048 deg/s at mid latitudes)
        let step = self.speed as f64 * 0.0000048 * dt_s;
        if dist > step * 2.0 {
            self.lat += dlat / dist * step;
            self.lon += dlon / dist * step;
        } else {
            // Arrived at waypoint, advance
            self.wp_idx = (self.wp_idx + 1) % self.waypoints.len();
        }

        // Small altitude jitter
        self.alt += rng.gen_range(-0.5..0.5);
        self.alt = self.alt.clamp(0.0, 200.0);
    }
}

// =============================================================================
// MISSION PHASE
// =============================================================================

fn phase_for_elapsed(elapsed_s: f64) -> &'static str {
    if elapsed_s < 30.0 {
        "planning"
    } else if elapsed_s < 120.0 {
        "transit"
    } else if elapsed_s < 240.0 {
        "engagement"
    } else {
        "rtb"
    }
}

// =============================================================================
// MAIN
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    info!("============================================");
    info!("  HDDS Mission Recorder - Simulation");
    info!("============================================");

    // -------------------------------------------------------------------------
    // DDS setup
    // -------------------------------------------------------------------------
    let participant = Participant::builder("mission_sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();
    let event_writer = participant.topic::<MissionEvent>(topics::MISSION_EVENT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let status_writer = participant.topic::<MissionStatus>(topics::MISSION_STATUS)?
        .writer()
        .qos(qos)
        .build()?;

    info!("DDS participant created, writers ready");

    // -------------------------------------------------------------------------
    // Initialize units around a patrol area (Toulon naval base vicinity)
    // -------------------------------------------------------------------------
    let mut units = vec![
        Unit::new("Alpha", vec![
            (43.095, 5.940), (43.110, 5.960), (43.120, 5.945), (43.100, 5.930),
        ]),
        Unit::new("Bravo", vec![
            (43.085, 5.950), (43.090, 5.970), (43.105, 5.975), (43.100, 5.955),
        ]),
        Unit::new("Charlie", vec![
            (43.100, 5.920), (43.115, 5.935), (43.120, 5.920), (43.105, 5.905),
        ]),
        Unit::new("Delta", vec![
            (43.080, 5.935), (43.075, 5.960), (43.085, 5.975), (43.095, 5.950),
        ]),
    ];

    // -------------------------------------------------------------------------
    // Simulation loop
    // -------------------------------------------------------------------------
    let mut rng = rand::thread_rng();
    let mut next_event_id: i64 = 1;
    let mut events_total: i64 = 0;

    let mission_start = Instant::now();
    let mut last_position_tick = Instant::now();
    let mut last_status_tick = Instant::now();

    let participant_ref = Arc::clone(&participant);
    info!(
        "Starting 5-minute mission ({} units), participant={}",
        units.len(),
        participant_ref.name()
    );

    loop {
        let elapsed = mission_start.elapsed();
        let elapsed_s = elapsed.as_secs_f64();

        if elapsed_s >= MISSION_DURATION_S {
            info!("Mission complete! {} events generated in {:.1}s", events_total, elapsed_s);
            break;
        }

        let now_ns = recorder_types::now_ns();
        let phase = phase_for_elapsed(elapsed_s);

        // -- Position updates (2 Hz) -----------------------------------------
        if last_position_tick.elapsed() >= POSITION_INTERVAL {
            last_position_tick = Instant::now();

            for unit in &mut units {
                unit.tick(POSITION_INTERVAL.as_secs_f64(), &mut rng);

                let evt = MissionEvent {
                    event_id: next_event_id,
                    event_type: "position".to_string(),
                    source: unit.name.to_string(),
                    latitude: unit.lat,
                    longitude: unit.lon,
                    altitude_m: unit.alt,
                    heading_deg: unit.heading,
                    speed_knots: unit.speed,
                    payload: "{}".to_string(),
                    timestamp_ns: now_ns,
                };
                if let Err(e) = event_writer.write(&evt) {
                    warn!("write error: {}", e);
                }
                next_event_id += 1;
                events_total += 1;
            }

            // Random contact event (~5% chance per tick during engagement)
            if phase == "engagement" && rng.gen::<f64>() < 0.05 {
                let src_idx = rng.gen_range(0..units.len());
                let u = &units[src_idx];
                let payload = serde_json::json!({
                    "contact_bearing": rng.gen_range(0.0_f64..360.0),
                    "contact_range_nm": rng.gen_range(0.5_f64..8.0),
                    "classification": "unknown_surface",
                });
                let evt = MissionEvent {
                    event_id: next_event_id,
                    event_type: "contact".to_string(),
                    source: u.name.to_string(),
                    latitude: u.lat,
                    longitude: u.lon,
                    altitude_m: u.alt,
                    heading_deg: u.heading,
                    speed_knots: u.speed,
                    payload: payload.to_string(),
                    timestamp_ns: now_ns,
                };
                if let Err(e) = event_writer.write(&evt) {
                    warn!("write error: {}", e);
                }
                next_event_id += 1;
                events_total += 1;
            }

            // Random comm event (~2% chance)
            if rng.gen::<f64>() < 0.02 {
                let src_idx = rng.gen_range(0..units.len());
                let u = &units[src_idx];
                let messages = [
                    "All stations, this is command, maintain course.",
                    "Contact report, bearing 045, range 3nm.",
                    "Request permission to engage.",
                    "Returning to base, bingo fuel.",
                    "Sonar contact, possible submarine.",
                ];
                let payload = serde_json::json!({
                    "message": messages[rng.gen_range(0..messages.len())],
                    "frequency_mhz": 243.0,
                });
                let evt = MissionEvent {
                    event_id: next_event_id,
                    event_type: "comm".to_string(),
                    source: u.name.to_string(),
                    latitude: u.lat,
                    longitude: u.lon,
                    altitude_m: u.alt,
                    heading_deg: u.heading,
                    speed_knots: u.speed,
                    payload: payload.to_string(),
                    timestamp_ns: now_ns,
                };
                if let Err(e) = event_writer.write(&evt) {
                    warn!("write error: {}", e);
                }
                next_event_id += 1;
                events_total += 1;
            }
        }

        // -- Status updates (0.5 Hz) -----------------------------------------
        if last_status_tick.elapsed() >= STATUS_INTERVAL {
            last_status_tick = Instant::now();

            let status = MissionStatus {
                mission_id: "MISSION-REC-001".to_string(),
                phase: phase.to_string(),
                elapsed_seconds: elapsed_s,
                events_recorded: events_total,
                participants_active: units.len() as i32,
                timestamp_ns: now_ns,
            };
            if let Err(e) = status_writer.write(&status) {
                warn!("status write error: {}", e);
            }

            info!(
                "[{:>6.1}s] phase={:<12} events={:<6} units={}",
                elapsed_s, phase, events_total, units.len()
            );
        }

        // Sleep to avoid busy-loop (~100 Hz internal rate)
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    Ok(())
}
