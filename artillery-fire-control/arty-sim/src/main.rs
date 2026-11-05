// CONCEPT DEMO - Simulated data for showcase purposes
//
// Artillery Fire Control Simulator
//
// Simulates a 3-battery artillery section with fire missions,
// ballistic computation, round tracking, and forward observer
// corrections over DDS topics via HDDS.
//
// Topics published:
//   arty/battery   RELIABLE+TRANSIENT_LOCAL  1 Hz    Battery status
//   arty/mission   RELIABLE                  event   Fire mission requests
//   arty/solution  RELIABLE                  event   Ballistic solutions
//   arty/round     BEST_EFFORT               10 Hz   Rounds in flight
//   arty/impact    RELIABLE                  event   Impact observations
//   arty/meteo     BEST_EFFORT               0.5 Hz  Weather data
//
// The hdds-ws bridge converts DDS/CDR to JSON over WebSocket for
// the arty-viewer web frontend.

use std::f64::consts::PI;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arty_types::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use tracing::info;

// ---------------------------------------------------------------------------
//  Constants
// ---------------------------------------------------------------------------

const GRAVITY: f64 = 9.81;
const MUZZLE_VELOCITY: f64 = 560.0; // m/s (M777 charge 5)
const BASE_CEP_M: f64 = 120.0; // circular error probable
const CORRECTION_FACTOR: f64 = 0.6; // each correction reduces CEP by this factor

// Battery positions (local XY in meters, origin bottom-left of 20km map)
const BATTERIES: [(&str, f64, f64, f64); 3] = [
    ("Alpha", 4000.0, 2000.0, 320.0),
    ("Bravo", 10000.0, 3000.0, 295.0),
    ("Charlie", 16000.0, 2500.0, 310.0),
];

// Possible target areas (XY in meters)
const TARGET_AREAS: [(f64, f64, f64, &str); 6] = [
    (5000.0, 16000.0, 180.0, "Hill 420 OBJ ALPHA"),
    (8000.0, 14000.0, 150.0, "Bridge complex NORTH"),
    (12000.0, 17000.0, 200.0, "Assembly area EAST"),
    (3000.0, 13000.0, 160.0, "Road junction WEST"),
    (15000.0, 15000.0, 190.0, "Supply depot CHARLIE"),
    (10000.0, 18000.0, 170.0, "OP THUNDER"),
];

const REQUESTING_UNITS: [&str; 4] = ["FO-11", "FO-22", "FO-33", "FSO-BN"];

// ---------------------------------------------------------------------------
//  Simulation state
// ---------------------------------------------------------------------------

struct Battery {
    id: String,
    x: f64,
    y: f64,
    alt: f64,
    state: BatteryState,
    rounds_remaining: u32,
    rounds_fired: u32,
    barrel_temp: f64,
    heading: f64,
    cooldown_ticks: u32,
}

impl Battery {
    fn new(id: &str, x: f64, y: f64, alt: f64) -> Self {
        Self {
            id: id.to_string(),
            x,
            y,
            alt,
            state: BatteryState::READY,
            rounds_remaining: 96,
            rounds_fired: 0,
            barrel_temp: 45.0,
            heading: 0.0,
            cooldown_ticks: 0,
        }
    }

    fn tick(&mut self) {
        // Barrel cooling
        if self.barrel_temp > 50.0 {
            self.barrel_temp -= 0.3;
        }
        // Cooldown timer
        if self.cooldown_ticks > 0 {
            self.cooldown_ticks -= 1;
            if self.cooldown_ticks == 0 {
                self.state = BatteryState::READY;
            }
        }
    }

    fn fire(&mut self) {
        self.rounds_remaining = self.rounds_remaining.saturating_sub(1);
        self.rounds_fired += 1;
        self.barrel_temp += 8.0;
        self.state = BatteryState::BATTERY_FIRING;
        self.cooldown_ticks = 30; // 3 seconds at 10Hz
    }
}

#[allow(dead_code)]
struct ActiveRound {
    round_id: u32,
    mission_id: u32,
    battery_id: String,
    // Launch position (kept for trajectory rendering)
    start_x: f64,
    start_y: f64,
    start_alt: f64,
    // Target position
    target_x: f64,
    target_y: f64,
    target_alt: f64,
    // Current position
    x: f64,
    y: f64,
    alt: f64,
    // Velocity components
    vx: f64,
    vy: f64,
    vz: f64,
    // Timing
    tof: f64,
    elapsed: f64,
    // Wind
    wind_x: f64,
    wind_y: f64,
    // Impact offset (pre-computed with CEP noise)
    impact_offset_x: f64,
    impact_offset_y: f64,
}

impl ActiveRound {
    fn is_airborne(&self) -> bool {
        self.elapsed < self.tof && self.alt >= 0.0
    }

    fn step(&mut self, dt: f64) {
        self.elapsed += dt;

        // Simple drag + gravity model
        let speed = (self.vx * self.vx + self.vy * self.vy + self.vz * self.vz).sqrt();
        let drag_coeff = 0.0001; // simplified
        let drag = drag_coeff * speed;

        self.vx -= drag * self.vx * dt + self.wind_x * 0.01 * dt;
        self.vy -= drag * self.vy * dt + self.wind_y * 0.01 * dt;
        self.vz -= GRAVITY * dt - drag * self.vz * dt;

        self.x += self.vx * dt;
        self.y += self.vy * dt;
        self.alt += self.vz * dt;
    }
}

struct ActiveMission {
    mission_id: u32,
    battery_idx: usize,
    target_x: f64,
    target_y: f64,
    target_alt: f64,
    target_desc: String,
    round_type: RoundType,
    rounds_requested: u32,
    rounds_fired: u32,
    status: MissionStatus,
    corrections: u32,
    current_cep: f64,
    // Ticks until next action
    delay_ticks: u32,
}

struct MeteoState {
    wind_speed: f64,
    wind_dir: f64,
    temperature: f64,
    pressure: f64,
    humidity: f64,
}

impl MeteoState {
    fn new() -> Self {
        Self {
            wind_speed: 4.5,
            wind_dir: 225.0,
            temperature: 18.0,
            pressure: 1013.25,
            humidity: 65.0,
        }
    }

    fn drift(&mut self, rng: &mut impl Rng) {
        self.wind_speed = (self.wind_speed + rng.gen_range(-0.3..0.3)).clamp(0.5, 15.0);
        self.wind_dir = (self.wind_dir + rng.gen_range(-3.0..3.0)) % 360.0;
        if self.wind_dir < 0.0 {
            self.wind_dir += 360.0;
        }
        self.temperature += rng.gen_range(-0.1..0.1);
        self.pressure += rng.gen_range(-0.2..0.2);
        self.humidity = (self.humidity + rng.gen_range(-0.5..0.5)).clamp(30.0, 95.0);
    }

    fn wind_x(&self) -> f64 {
        self.wind_speed * (self.wind_dir * PI / 180.0).sin()
    }

    fn wind_y(&self) -> f64 {
        self.wind_speed * (self.wind_dir * PI / 180.0).cos()
    }

    fn air_density(&self) -> f64 {
        // Simplified air density from temperature and pressure
        let t_kelvin = self.temperature + 273.15;
        self.pressure * 100.0 / (287.05 * t_kelvin)
    }
}

// ---------------------------------------------------------------------------
//  Ballistic computation
// ---------------------------------------------------------------------------

fn compute_ballistic(
    battery: &Battery,
    target_x: f64,
    target_y: f64,
    _target_alt: f64,
    meteo: &MeteoState,
) -> (f64, f64, f64, f64, f64) {
    // Returns: (azimuth_deg, elevation_deg, tof_s, max_alt_m, range_m)
    let dx = target_x - battery.x;
    let dy = target_y - battery.y;
    let range = (dx * dx + dy * dy).sqrt();
    let azimuth = dy.atan2(dx) * 180.0 / PI;

    // Simplified elevation from range and muzzle velocity
    // Using: sin(2*theta) = g*R / v^2  =>  theta = 0.5 * asin(g*R/v^2)
    let arg = (GRAVITY * range) / (MUZZLE_VELOCITY * MUZZLE_VELOCITY);
    let elevation = if arg <= 1.0 {
        0.5 * arg.asin() * 180.0 / PI
    } else {
        45.0 // max range, lob it
    };

    let elevation_rad = elevation * PI / 180.0;
    let _vx_init = MUZZLE_VELOCITY * elevation_rad.cos();
    let vz_init = MUZZLE_VELOCITY * elevation_rad.sin();

    // Time of flight: t = 2 * vz / g (simplified, no drag)
    let tof = 2.0 * vz_init / GRAVITY;

    // Max altitude
    let max_alt = battery.alt + (vz_init * vz_init) / (2.0 * GRAVITY);

    // Wind correction in mils (1 mil ~ 0.001 rad ~ 1m at 1000m)
    let _wind_cross = meteo.wind_x() * (azimuth * PI / 180.0).cos()
        - meteo.wind_y() * (azimuth * PI / 180.0).sin();

    (azimuth, elevation, tof, max_alt, range)
}

fn gaussian_noise(rng: &mut impl Rng) -> f64 {
    // Box-Muller transform
    let u1: f64 = rng.gen_range(0.001..1.0);
    let u2: f64 = rng.gen_range(0.0..1.0);
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

// ---------------------------------------------------------------------------
//  Main
// ---------------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    println!("============================================================");
    println!("  HDDS Artillery Fire Control Simulator");
    println!("  3 batteries | ballistic model | forward observer loop");
    println!("============================================================");
    println!();

    // --- DDS setup ---

    let participant = Participant::builder("arty-sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS participant created on domain 0");

    let battery_writer = participant.topic::<BatteryStatus>(topics::BATTERY_STATUS)?
        .writer()
        .qos(QoS::reliable().transient_local())
        .build()?;
    let mission_writer = participant.topic::<FireMission>(topics::FIRE_MISSION)?
        .writer()
        .qos(QoS::reliable())
        .build()?;
    let solution_writer = participant.topic::<BallisticSolution>(topics::BALLISTIC_SOLUTION)?
        .writer()
        .qos(QoS::reliable())
        .build()?;
    let round_writer = participant.topic::<RoundInFlight>(topics::ROUND_IN_FLIGHT)?
        .writer()
        .qos(QoS::best_effort())
        .build()?;
    let impact_writer = participant.topic::<ImpactObservation>(topics::IMPACT_OBSERVATION)?
        .writer()
        .qos(QoS::reliable())
        .build()?;
    let meteo_writer = participant.topic::<MeteoData>(topics::METEO_DATA)?
        .writer()
        .qos(QoS::best_effort())
        .build()?;

    info!("6 writers created (battery, mission, solution, round, impact, meteo)");
    println!();

    // --- Simulation state ---

    let mut batteries: Vec<Battery> = BATTERIES
        .iter()
        .map(|(id, x, y, alt)| Battery::new(id, *x, *y, *alt))
        .collect();

    let mut meteo = MeteoState::new();
    let mut rng = rand::thread_rng();

    let mut active_rounds: Vec<ActiveRound> = Vec::new();
    let mut active_missions: Vec<ActiveMission> = Vec::new();

    let mut next_mission_id: u32 = 1;
    let mut next_round_id: u32 = 1;

    let mut tick: u64 = 0;
    let mut mission_cooldown: u32 = 50; // ticks until first mission (~5s)

    // --- Main loop at 10Hz ---

    loop {
        tick += 1;
        let ts = now_ns();
        let dt = 0.1; // 100ms

        // --- Tick batteries ---
        for bat in &mut batteries {
            bat.tick();
        }

        // --- Meteo drift every 20 ticks (2s) ---
        if tick % 20 == 0 {
            meteo.drift(&mut rng);
        }

        // --- Publish meteo at 0.5Hz (every 20 ticks) ---
        if tick % 20 == 0 {
            let _ = meteo_writer.write(&MeteoData {
                station_id: "METEO-01".to_string(),
                wind_speed_mps: meteo.wind_speed,
                wind_dir_deg: meteo.wind_dir,
                air_density: meteo.air_density(),
                temperature_c: meteo.temperature,
                pressure_hpa: meteo.pressure,
                humidity_pct: meteo.humidity,
                timestamp_ns: ts,
            });
        }

        // --- Publish battery status at 1Hz (every 10 ticks) ---
        if tick % 10 == 0 {
            for bat in &batteries {
                let _ = battery_writer.write(&BatteryStatus {
                    battery_id: bat.id.clone(),
                    lat_deg: bat.x, // using XY as "lat/lon" for simplicity
                    lon_deg: bat.y,
                    alt_m: bat.alt,
                    state: bat.state,
                    rounds_remaining: bat.rounds_remaining,
                    rounds_fired: bat.rounds_fired,
                    barrel_temp_c: bat.barrel_temp,
                    heading_deg: bat.heading,
                    timestamp_ns: ts,
                });
            }
        }

        // --- Generate new fire missions ---
        if mission_cooldown > 0 {
            mission_cooldown -= 1;
        }
        if mission_cooldown == 0 {
            // New mission every 15-20 seconds
            mission_cooldown = rng.gen_range(150..200);

            let target_idx = rng.gen_range(0..TARGET_AREAS.len());
            let (tx, ty, ta, desc) = TARGET_AREAS[target_idx];

            // Add some jitter to target position
            let tx = tx + rng.gen_range(-300.0..300.0);
            let ty = ty + rng.gen_range(-300.0..300.0);

            // Pick the closest ready battery
            let battery_idx = batteries
                .iter()
                .enumerate()
                .filter(|(_, b)| b.state == BatteryState::READY && b.rounds_remaining > 0)
                .min_by(|(_, a), (_, b)| {
                    let da = ((a.x - tx).powi(2) + (a.y - ty).powi(2)).sqrt();
                    let db = ((b.x - tx).powi(2) + (b.y - ty).powi(2)).sqrt();
                    da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i);

            if let Some(bi) = battery_idx {
                let rounds_req = rng.gen_range(2..6);
                let round_type = match rng.gen_range(0u32..10) {
                    0..=6 => RoundType::HE,
                    7..=8 => RoundType::SMOKE,
                    _ => RoundType::ILLUM,
                };
                let requesting = REQUESTING_UNITS[rng.gen_range(0..REQUESTING_UNITS.len())];

                let mid = next_mission_id;
                next_mission_id += 1;

                info!(
                    "FIRE MISSION #{} from {} -> {} ({} x {})",
                    mid, requesting, desc, rounds_req, round_type.name()
                );

                let _ = mission_writer.write(&FireMission {
                    mission_id: mid,
                    requesting_unit: requesting.to_string(),
                    target_description: desc.to_string(),
                    target_lat: tx,
                    target_lon: ty,
                    target_alt: ta,
                    round_type,
                    rounds_requested: rounds_req,
                    status: MissionStatus::COMPUTING,
                    timestamp_ns: ts,
                });

                active_missions.push(ActiveMission {
                    mission_id: mid,
                    battery_idx: bi,
                    target_x: tx,
                    target_y: ty,
                    target_alt: ta,
                    target_desc: desc.to_string(),
                    round_type,
                    rounds_requested: rounds_req,
                    rounds_fired: 0,
                    status: MissionStatus::COMPUTING,
                    corrections: 0,
                    current_cep: BASE_CEP_M,
                    delay_ticks: 20, // 2 seconds to compute solution
                });
            }
        }

        // --- Process active missions ---
        let mut new_rounds: Vec<ActiveRound> = Vec::new();

        for mission in &mut active_missions {
            if mission.status == MissionStatus::COMPLETE
                || mission.status == MissionStatus::CANCELLED
            {
                continue;
            }

            if mission.delay_ticks > 0 {
                mission.delay_ticks -= 1;
                continue;
            }

            match mission.status {
                MissionStatus::COMPUTING => {
                    // Publish ballistic solution
                    let bat_id = batteries[mission.battery_idx].id.clone();
                    let (az, el, tof, max_alt, range) = compute_ballistic(
                        &batteries[mission.battery_idx],
                        mission.target_x,
                        mission.target_y,
                        mission.target_alt,
                        &meteo,
                    );

                    // Update battery heading toward target
                    batteries[mission.battery_idx].heading = az;

                    let wind_correction = meteo.wind_speed * tof * 0.05; // simplified

                    let _ = solution_writer.write(&BallisticSolution {
                        mission_id: mission.mission_id,
                        battery_id: bat_id,
                        azimuth_deg: az,
                        elevation_deg: el,
                        charge_vel_mps: MUZZLE_VELOCITY,
                        time_of_flight_s: tof,
                        max_alt_m: max_alt,
                        range_m: range,
                        deflection_mil: 0.0,
                        wind_correction_mil: wind_correction,
                        timestamp_ns: ts,
                    });

                    info!(
                        "  Solution: az={:.1} el={:.1} tof={:.1}s range={:.0}m",
                        az, el, tof, range
                    );

                    mission.status = MissionStatus::MISSION_FIRING;
                    mission.delay_ticks = 10; // 1 second before first round
                }
                MissionStatus::MISSION_FIRING => {
                    if mission.rounds_fired >= mission.rounds_requested {
                        // All rounds fired, wait for last impact
                        mission.delay_ticks = 30;
                        mission.status = MissionStatus::ADJUSTING;
                        continue;
                    }

                    let bat = &batteries[mission.battery_idx];
                    if bat.state != BatteryState::READY || bat.rounds_remaining == 0 {
                        mission.delay_ticks = 5;
                        continue;
                    }

                    // Fire a round
                    let (az, el, tof, _max_alt, _range) = compute_ballistic(
                        bat,
                        mission.target_x,
                        mission.target_y,
                        mission.target_alt,
                        &meteo,
                    );

                    let el_rad = el * PI / 180.0;
                    let az_rad = az * PI / 180.0;

                    let vx = MUZZLE_VELOCITY * el_rad.cos() * az_rad.cos();
                    let vy = MUZZLE_VELOCITY * el_rad.cos() * az_rad.sin();
                    let vz = MUZZLE_VELOCITY * el_rad.sin();

                    // Pre-compute impact error using gaussian noise + current CEP
                    let err_x = gaussian_noise(&mut rng) * mission.current_cep * 0.5;
                    let err_y = gaussian_noise(&mut rng) * mission.current_cep * 0.5;

                    let rid = next_round_id;
                    next_round_id += 1;

                    new_rounds.push(ActiveRound {
                        round_id: rid,
                        mission_id: mission.mission_id,
                        battery_id: bat.id.clone(),
                        start_x: bat.x,
                        start_y: bat.y,
                        start_alt: bat.alt,
                        target_x: mission.target_x + err_x,
                        target_y: mission.target_y + err_y,
                        target_alt: mission.target_alt,
                        x: bat.x,
                        y: bat.y,
                        alt: bat.alt,
                        vx,
                        vy,
                        vz,
                        tof,
                        elapsed: 0.0,
                        wind_x: meteo.wind_x(),
                        wind_y: meteo.wind_y(),
                        impact_offset_x: err_x,
                        impact_offset_y: err_y,
                    });

                    batteries[mission.battery_idx].fire();
                    mission.rounds_fired += 1;
                    mission.delay_ticks = 15; // 1.5s between rounds

                    info!(
                        "  SHOT! Round #{} mission #{} ({}/{})",
                        rid, mission.mission_id, mission.rounds_fired, mission.rounds_requested
                    );
                }
                MissionStatus::ADJUSTING => {
                    // Forward observer sends correction
                    let err_range = gaussian_noise(&mut rng) * mission.current_cep * 0.3;
                    let err_deflection = gaussian_noise(&mut rng) * mission.current_cep * 0.3;

                    let destroyed = mission.corrections >= 1
                        && rng.gen_ratio(
                            (mission.corrections as u32).min(3),
                            4,
                        );

                    let correction_text = if destroyed {
                        "Target destroyed, end of mission".to_string()
                    } else {
                        format!(
                            "Adjust fire, range {:+.0}m, deflection {:+.0}m",
                            err_range, err_deflection
                        )
                    };

                    let _ = impact_writer.write(&ImpactObservation {
                        mission_id: mission.mission_id,
                        observer_id: "FO-11".to_string(),
                        impact_lat: mission.target_x + err_range,
                        impact_lon: mission.target_y + err_deflection,
                        error_range_m: err_range.abs(),
                        error_deflection_m: err_deflection.abs(),
                        correction: correction_text.clone(),
                        target_destroyed: destroyed,
                        timestamp_ns: ts,
                    });

                    info!(
                        "  OBS mission #{}: {}",
                        mission.mission_id, correction_text
                    );

                    if destroyed {
                        mission.status = MissionStatus::COMPLETE;
                        let _ = mission_writer.write(&FireMission {
                            mission_id: mission.mission_id,
                            requesting_unit: String::new(),
                            target_description: mission.target_desc.clone(),
                            target_lat: mission.target_x,
                            target_lon: mission.target_y,
                            target_alt: mission.target_alt,
                            round_type: mission.round_type,
                            rounds_requested: mission.rounds_requested,
                            status: MissionStatus::COMPLETE,
                            timestamp_ns: ts,
                        });
                    } else {
                        // Apply correction, reduce CEP, fire again
                        mission.corrections += 1;
                        mission.current_cep *= CORRECTION_FACTOR;
                        mission.rounds_fired = 0;
                        mission.rounds_requested = rng.gen_range(1..4);
                        mission.status = MissionStatus::COMPUTING;
                        mission.delay_ticks = 20; // 2s to recompute

                        let _ = mission_writer.write(&FireMission {
                            mission_id: mission.mission_id,
                            requesting_unit: String::new(),
                            target_description: mission.target_desc.clone(),
                            target_lat: mission.target_x,
                            target_lon: mission.target_y,
                            target_alt: mission.target_alt,
                            round_type: mission.round_type,
                            rounds_requested: mission.rounds_requested,
                            status: MissionStatus::ADJUSTING,
                            timestamp_ns: ts,
                        });
                    }
                }
                _ => {}
            }
        }

        // Add newly launched rounds
        active_rounds.extend(new_rounds);

        // --- Update and publish rounds in flight ---
        let mut impacts_to_report: Vec<(u32, f64, f64, f64, f64)> = Vec::new();

        for round in &mut active_rounds {
            if !round.is_airborne() {
                continue;
            }

            round.step(dt);

            // Publish tracking data
            let speed = (round.vx.powi(2) + round.vy.powi(2) + round.vz.powi(2)).sqrt();
            let time_remaining = (round.tof - round.elapsed).max(0.0);

            let _ = round_writer.write(&RoundInFlight {
                round_id: round.round_id,
                mission_id: round.mission_id,
                battery_id: round.battery_id.clone(),
                lat_deg: round.x,
                lon_deg: round.y,
                alt_m: round.alt,
                velocity_mps: speed,
                time_remaining_s: time_remaining,
                timestamp_ns: ts,
            });

            // Check for impact
            if round.alt <= round.target_alt || round.elapsed >= round.tof {
                impacts_to_report.push((
                    round.mission_id,
                    round.x,
                    round.y,
                    round.impact_offset_x,
                    round.impact_offset_y,
                ));
            }
        }

        // Remove landed rounds
        active_rounds.retain(|r| r.is_airborne());

        // Clean up completed missions (keep last 20 for display)
        if active_missions.len() > 20 {
            active_missions.retain(|m| {
                m.status != MissionStatus::COMPLETE && m.status != MissionStatus::CANCELLED
            });
        }

        // --- Console output at 1Hz ---
        if tick % 10 == 0 {
            let active = active_missions
                .iter()
                .filter(|m| {
                    m.status != MissionStatus::COMPLETE && m.status != MissionStatus::CANCELLED
                })
                .count();
            let completed = active_missions
                .iter()
                .filter(|m| m.status == MissionStatus::COMPLETE)
                .count();

            println!(
                "  [T+{:>4}s] Btry: {} {} {} | Missions: {} active, {} done | Rounds airborne: {} | Wind: {:.1}m/s @ {:.0}deg",
                tick / 10,
                batteries[0].state.name(),
                batteries[1].state.name(),
                batteries[2].state.name(),
                active,
                completed,
                active_rounds.len(),
                meteo.wind_speed,
                meteo.wind_dir,
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
