//! Air Defense Radar Simulator
//!
//! Generates simulated radar data:
//! - Radar sweep (rotating at 15 RPM = 90°/s)
//! - Aircraft tracks (mix of friendly, hostile, neutral)
//! - SAM sites with engagement logic
//! - Engagements and results

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use log::*;
use radar_types::*;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// Simulation constants
const RADAR_RANGE_KM: f32 = 200.0;      // Radar detection range
const SWEEP_RPM: f32 = 15.0;            // Rotations per minute
const SWEEP_RATE: f32 = SWEEP_RPM * 6.0; // Degrees per second (90°/s)
const SIM_TICK_MS: u64 = 100;           // Simulation tick rate (10 Hz)

const MIN_AIRCRAFT: usize = 5;
const MAX_AIRCRAFT: usize = 12;
const SPAWN_INTERVAL_S: f32 = 8.0;      // New aircraft spawn interval

/// Simulated aircraft state
struct Aircraft {
    track: AircraftTrack,
    spawn_time: Instant,
    ttl_s: f32,                         // Time to live before leaving
}

impl Aircraft {
    fn new(track_id: u32, rng: &mut impl Rng) -> Self {
        // Spawn at edge of radar range, heading toward center
        let spawn_angle = rng.gen_range(0.0..360.0f32).to_radians();
        let spawn_dist = RADAR_RANGE_KM * 0.9;

        let x = spawn_dist * spawn_angle.sin();
        let y = spawn_dist * spawn_angle.cos();

        // Head toward center with some randomness
        let heading = (spawn_angle.to_degrees() + 180.0 + rng.gen_range(-30.0..30.0)) % 360.0;

        // Random status (weighted: 40% friendly, 40% hostile, 20% neutral)
        let status = match rng.gen_range(0..10) {
            0..=3 => track_status::FRIENDLY,
            4..=7 => track_status::HOSTILE,
            _ => track_status::NEUTRAL,
        };

        // Generate callsign
        let prefix = match status {
            track_status::FRIENDLY => "FR",
            track_status::HOSTILE => "HO",
            track_status::NEUTRAL => "NT",
            _ => "UK",
        };
        let callsign = format!("{}-{:04}", prefix, rng.gen_range(1000..9999));

        Aircraft {
            track: AircraftTrack {
                track_id,
                x_km: x,
                y_km: y,
                altitude_m: rng.gen_range(5000.0..12000.0),
                heading_deg: heading,
                speed_kts: rng.gen_range(300.0..600.0),
                status,
                callsign,
                rcs: rng.gen_range(-10.0..20.0), // dBsm
                timestamp_us: timestamp_us(),
            },
            spawn_time: Instant::now(),
            ttl_s: rng.gen_range(60.0..180.0),
        }
    }

    fn update(&mut self, dt_s: f32) {
        // Convert speed from knots to km/s
        let speed_kms = self.track.speed_kts * 0.000514444;

        // Move in heading direction
        let heading_rad = self.track.heading_deg.to_radians();
        self.track.x_km += speed_kms * heading_rad.sin() * dt_s;
        self.track.y_km += speed_kms * heading_rad.cos() * dt_s;

        // Small heading variation
        self.track.heading_deg += rand::thread_rng().gen_range(-0.5..0.5);
        self.track.heading_deg = self.track.heading_deg.rem_euclid(360.0);

        self.track.timestamp_us = timestamp_us();
    }

    fn is_in_range(&self) -> bool {
        let dist = (self.track.x_km.powi(2) + self.track.y_km.powi(2)).sqrt();
        dist < RADAR_RANGE_KM * 1.1
    }

    fn is_expired(&self) -> bool {
        self.spawn_time.elapsed().as_secs_f32() > self.ttl_s
    }
}

/// SAM site state
struct SamSiteState {
    site: SamSite,
    cooldown_until: Option<Instant>,
    engaging_track: Option<u32>,
}

impl SamSiteState {
    fn new(site_id: u32, x: f32, y: f32, name: &str, range: f32) -> Self {
        SamSiteState {
            site: SamSite {
                site_id,
                x_km: x,
                y_km: y,
                range_km: range,
                missiles_ready: 8,
                missiles_total: 8,
                site_name: name.to_string(),
                active: 1,
            },
            cooldown_until: None,
            engaging_track: None,
        }
    }

    fn can_engage(&self) -> bool {
        self.site.active == 1
            && self.site.missiles_ready > 0
            && self.engaging_track.is_none()
            && self.cooldown_until.map(|t| Instant::now() > t).unwrap_or(true)
    }

    fn is_in_range(&self, aircraft: &Aircraft) -> bool {
        let dx = aircraft.track.x_km - self.site.x_km;
        let dy = aircraft.track.y_km - self.site.y_km;
        let dist = (dx * dx + dy * dy).sqrt();
        dist < self.site.range_km
    }

    fn start_engagement(&mut self, track_id: u32) {
        self.engaging_track = Some(track_id);
        self.site.missiles_ready = self.site.missiles_ready.saturating_sub(1);
    }

    fn end_engagement(&mut self) {
        self.engaging_track = None;
        self.cooldown_until = Some(Instant::now() + Duration::from_secs(5));
    }
}

/// Active engagement state
struct EngagementState {
    engagement: Engagement,
    start_time: Instant,
    flight_time_s: f32,
    target_x: f32,
    target_y: f32,
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  Air Defense Radar Simulator");
    info!("  HDDS Demo - PC Only");
    info!("==========================================");

    // Create DDS participant
    let participant = Participant::builder("radar_simulator")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    // Create writers
    let sweep_writer = participant.topic::<RadarSweep>(topics::RADAR_SWEEP)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let track_writer = participant.topic::<AircraftTrack>(topics::AIRCRAFT_TRACK)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let site_writer = participant.topic::<SamSite>(topics::SAM_SITE)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let engagement_writer = participant.topic::<Engagement>(topics::ENGAGEMENT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let result_writer = participant.topic::<EngagementResult>(topics::ENGAGEMENT_RESULT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let alert_writer = participant.topic::<RadarAlert>(topics::RADAR_ALERT)?
        .writer()
        .qos(qos)
        .build()?;

    info!("DDS participant ready");
    info!("  Radar range: {} km", RADAR_RANGE_KM);
    info!("  Sweep rate: {} RPM ({} °/s)", SWEEP_RPM, SWEEP_RATE);

    // Initialize simulation state
    let mut rng = rand::thread_rng();
    let mut sweep_angle: f32 = 0.0;
    let mut next_track_id: u32 = 1;
    let mut next_engagement_id: u32 = 1;
    let mut next_alert_id: u32 = 1;

    // Aircraft
    let mut aircraft: HashMap<u32, Aircraft> = HashMap::new();
    let mut last_spawn = Instant::now();

    // SAM sites (3 sites around the radar)
    let mut sam_sites = vec![
        SamSiteState::new(1, -50.0, 30.0, "PATRIOT-1", 80.0),
        SamSiteState::new(2, 40.0, 40.0, "PATRIOT-2", 80.0),
        SamSiteState::new(3, 0.0, -60.0, "S-300", 120.0),
    ];

    // Active engagements
    let mut engagements: HashMap<u32, EngagementState> = HashMap::new();

    // Publish initial SAM sites
    for site_state in &sam_sites {
        site_writer.write(&site_state.site)?;
    }
    info!("Published {} SAM sites", sam_sites.len());

    // Spawn initial aircraft
    for _ in 0..rng.gen_range(MIN_AIRCRAFT..MAX_AIRCRAFT) {
        let ac = Aircraft::new(next_track_id, &mut rng);
        info!("Spawned aircraft {} ({:?}) at ({:.1}, {:.1})",
              ac.track.callsign, ac.track.status, ac.track.x_km, ac.track.y_km);
        aircraft.insert(next_track_id, ac);
        next_track_id += 1;
    }

    let mut last_tick = Instant::now();

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Update sweep angle
        sweep_angle = (sweep_angle + SWEEP_RATE * dt) % 360.0;
        sweep_writer.write(&RadarSweep {
            radar_id: 1,
            azimuth_deg: sweep_angle,
            range_km: RADAR_RANGE_KM,
            timestamp_us: timestamp_us(),
        })?;

        // Spawn new aircraft periodically
        if now.duration_since(last_spawn).as_secs_f32() > SPAWN_INTERVAL_S
            && aircraft.len() < MAX_AIRCRAFT
        {
            let ac = Aircraft::new(next_track_id, &mut rng);
            let is_hostile = ac.track.status == track_status::HOSTILE;
            info!("New aircraft {} ({:?})", ac.track.callsign, ac.track.status);

            if is_hostile {
                alert_writer.write(&RadarAlert {
                    alert_id: next_alert_id,
                    track_id: next_track_id,
                    alert_type: "NEW_HOSTILE".to_string(),
                    message: format!("New hostile contact: {}", ac.track.callsign),
                    priority: 1,
                    timestamp_us: timestamp_us(),
                })?;
                next_alert_id += 1;
            }

            aircraft.insert(next_track_id, ac);
            next_track_id += 1;
            last_spawn = now;
        }

        // Update and publish aircraft tracks
        let mut to_remove = Vec::new();
        for (id, ac) in aircraft.iter_mut() {
            ac.update(dt);
            track_writer.write(&ac.track)?;

            if !ac.is_in_range() || ac.is_expired() {
                to_remove.push(*id);
            }
        }

        // Remove aircraft that left the area
        for id in to_remove {
            if let Some(ac) = aircraft.remove(&id) {
                info!("Aircraft {} left radar coverage", ac.track.callsign);
            }
        }

        // SAM engagement logic
        for site in sam_sites.iter_mut() {
            if !site.can_engage() {
                continue;
            }

            // Find hostile aircraft in range
            for (id, ac) in aircraft.iter() {
                if ac.track.status != track_status::HOSTILE {
                    continue;
                }
                if !site.is_in_range(ac) {
                    continue;
                }

                // Check if already being engaged
                let already_engaged = engagements.values().any(|e| e.engagement.track_id == *id);
                if already_engaged {
                    continue;
                }

                // Start engagement
                site.start_engagement(*id);
                let engagement_id = next_engagement_id;
                next_engagement_id += 1;

                let flight_time = 8.0 + rng.gen_range(0.0..4.0); // 8-12 seconds

                let engagement = Engagement {
                    engagement_id,
                    site_id: site.site.site_id,
                    track_id: *id,
                    phase: engagement_phase::LAUNCHING,
                    missile_x_km: site.site.x_km,
                    missile_y_km: site.site.y_km,
                    missile_alt_m: 0.0,
                    intercept_x_km: ac.track.x_km,
                    intercept_y_km: ac.track.y_km,
                    time_to_intercept_s: flight_time,
                    timestamp_us: timestamp_us(),
                };

                info!("SAM {} engaging {} (flight time: {:.1}s)",
                      site.site.site_name, ac.track.callsign, flight_time);

                engagement_writer.write(&engagement)?;

                engagements.insert(engagement_id, EngagementState {
                    engagement: engagement,
                    start_time: now,
                    flight_time_s: flight_time,
                    target_x: ac.track.x_km,
                    target_y: ac.track.y_km,
                });

                // Alert
                alert_writer.write(&RadarAlert {
                    alert_id: next_alert_id,
                    track_id: *id,
                    alert_type: "SAM_LAUNCH".to_string(),
                    message: format!("{} engaging {}", site.site.site_name, ac.track.callsign),
                    priority: 2,
                    timestamp_us: timestamp_us(),
                })?;
                next_alert_id += 1;

                break; // One engagement per tick per site
            }
        }

        // Update engagements
        let mut completed_engagements = Vec::new();
        for (id, state) in engagements.iter_mut() {
            let elapsed = now.duration_since(state.start_time).as_secs_f32();
            let progress = (elapsed / state.flight_time_s).min(1.0);

            // Update missile position (lerp from site to target)
            let site = sam_sites.iter().find(|s| s.site.site_id == state.engagement.site_id);
            if let Some(site) = site {
                state.engagement.missile_x_km = site.site.x_km + (state.target_x - site.site.x_km) * progress;
                state.engagement.missile_y_km = site.site.y_km + (state.target_y - site.site.y_km) * progress;
                state.engagement.missile_alt_m = 5000.0 * (1.0 - (progress - 0.5).abs() * 2.0).max(0.0);
                state.engagement.time_to_intercept_s = (state.flight_time_s - elapsed).max(0.0);

                state.engagement.phase = if progress < 0.3 {
                    engagement_phase::LAUNCHING
                } else if progress < 1.0 {
                    engagement_phase::INTERCEPTING
                } else {
                    engagement_phase::COMPLETE
                };

                state.engagement.timestamp_us = timestamp_us();
                engagement_writer.write(&state.engagement)?;
            }

            if progress >= 1.0 {
                completed_engagements.push(*id);
            }
        }

        // Process completed engagements
        for id in completed_engagements {
            if let Some(state) = engagements.remove(&id) {
                // 80% hit probability
                let success = rng.gen_bool(0.8);

                let result = EngagementResult {
                    engagement_id: id,
                    track_id: state.engagement.track_id,
                    site_id: state.engagement.site_id,
                    success: if success { 1 } else { 0 },
                    details: if success {
                        "Target destroyed".to_string()
                    } else {
                        "Missile missed".to_string()
                    },
                    timestamp_us: timestamp_us(),
                };

                result_writer.write(&result)?;

                // Update site state
                if let Some(site) = sam_sites.iter_mut().find(|s| s.site.site_id == state.engagement.site_id) {
                    site.end_engagement();
                    site_writer.write(&site.site)?;
                }

                if success {
                    // Remove destroyed aircraft
                    if let Some(ac) = aircraft.remove(&state.engagement.track_id) {
                        info!("Aircraft {} destroyed by SAM", ac.track.callsign);
                    }

                    alert_writer.write(&RadarAlert {
                        alert_id: next_alert_id,
                        track_id: state.engagement.track_id,
                        alert_type: "TARGET_DESTROYED".to_string(),
                        message: "Hostile target destroyed".to_string(),
                        priority: 1,
                        timestamp_us: timestamp_us(),
                    })?;
                } else {
                    info!("SAM missile missed target");
                    alert_writer.write(&RadarAlert {
                        alert_id: next_alert_id,
                        track_id: state.engagement.track_id,
                        alert_type: "MISSILE_MISS".to_string(),
                        message: "SAM missile missed target".to_string(),
                        priority: 1,
                        timestamp_us: timestamp_us(),
                    })?;
                }
                next_alert_id += 1;
            }
        }

        tokio::time::sleep(Duration::from_millis(SIM_TICK_MS)).await;
    }
}
