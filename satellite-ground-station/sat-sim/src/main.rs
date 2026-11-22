// CONCEPT DEMO - Simulated data for showcase purposes
//
// Satellite Ground Station Network Simulator
//
// Simulates a constellation of 8 satellites in various orbits and
// 3 ground stations tracking them. Publishes satellite positions,
// station status, pass predictions, telemetry frames, and orbital
// alerts via HDDS DDS topics.
//
// Topics published:
//   sat/position   BEST_EFFORT  1 Hz  Satellite positions (8 sats)
//   sat/station    BEST_EFFORT  1 Hz  Ground station status (3 stations)
//   sat/pass       RELIABLE     event Pass predictions (AOS/LOS events)
//   sat/telemetry  BEST_EFFORT  1 Hz  Telemetry frames during passes
//   sat/alert      RELIABLE     event Orbital alerts (conjunction warnings)

use std::f64::consts::PI;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use sat_types::*;
use tracing::info;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// =============================================================================
// CONSTANTS
// =============================================================================

const EARTH_ROTATION_DEG_PER_SEC: f64 = 360.0 / 86164.0; // sidereal day
const ELEVATION_THRESHOLD_DEG: f64 = 5.0;
const EARTH_RADIUS_KM: f64 = 6371.0;

// =============================================================================
// SATELLITE DEFINITION
// =============================================================================

struct SatDef {
    norad_id: &'static str,
    name: &'static str,
    inclination_deg: f64,
    alt_km: f64,
    period_min: f64,
    initial_angle_deg: f64,
    initial_lon_deg: f64,
}

const SATELLITES: &[SatDef] = &[
    SatDef {
        norad_id: "25544",
        name: "ISS",
        inclination_deg: 51.6,
        alt_km: 408.0,
        period_min: 92.5,
        initial_angle_deg: 0.0,
        initial_lon_deg: -30.0,
    },
    SatDef {
        norad_id: "20580",
        name: "Hubble",
        inclination_deg: 28.5,
        alt_km: 547.0,
        period_min: 95.5,
        initial_angle_deg: 45.0,
        initial_lon_deg: 60.0,
    },
    SatDef {
        norad_id: "28874",
        name: "GPS-IIR-14",
        inclination_deg: 55.0,
        alt_km: 20200.0,
        period_min: 718.0,
        initial_angle_deg: 0.0,
        initial_lon_deg: -120.0,
    },
    SatDef {
        norad_id: "40294",
        name: "GPS-IIF-09",
        inclination_deg: 55.0,
        alt_km: 20200.0,
        period_min: 718.0,
        initial_angle_deg: 120.0,
        initial_lon_deg: 0.0,
    },
    SatDef {
        norad_id: "41328",
        name: "GPS-IIF-12",
        inclination_deg: 55.0,
        alt_km: 20200.0,
        period_min: 718.0,
        initial_angle_deg: 240.0,
        initial_lon_deg: 120.0,
    },
    SatDef {
        norad_id: "44713",
        name: "Starlink-1007",
        inclination_deg: 53.0,
        alt_km: 550.0,
        period_min: 95.0,
        initial_angle_deg: 90.0,
        initial_lon_deg: -80.0,
    },
    SatDef {
        norad_id: "44714",
        name: "Starlink-1008",
        inclination_deg: 53.0,
        alt_km: 550.0,
        period_min: 95.0,
        initial_angle_deg: 180.0,
        initial_lon_deg: 100.0,
    },
    SatDef {
        norad_id: "36744",
        name: "GOES-16",
        inclination_deg: 0.0,
        alt_km: 35786.0,
        period_min: 1436.0,
        initial_angle_deg: 0.0,
        initial_lon_deg: -75.2,
    },
];

// =============================================================================
// GROUND STATION DEFINITION
// =============================================================================

struct StationDef {
    id: &'static str,
    name: &'static str,
    lat_deg: f64,
    lon_deg: f64,
    alt_m: f64,
}

const STATIONS: &[StationDef] = &[
    StationDef {
        id: "SVB",
        name: "Svalbard SvalSat",
        lat_deg: 78.2,
        lon_deg: 15.6,
        alt_m: 500.0,
    },
    StationDef {
        id: "KOU",
        name: "Kourou CSG",
        lat_deg: 5.2,
        lon_deg: -52.8,
        alt_m: 15.0,
    },
    StationDef {
        id: "CAN",
        name: "Canberra DSN",
        lat_deg: -35.3,
        lon_deg: 149.0,
        alt_m: 680.0,
    },
];

// =============================================================================
// SIMULATION STATE
// =============================================================================

struct SatState {
    def_idx: usize,
    angle_rad: f64,
    status: SatStatus,
    frame_seq: u32,
}

struct StationState {
    def_idx: usize,
    tracking_sat: Option<usize>,
    antenna_az_deg: f64,
    antenna_el_deg: f64,
    signal_db: f64,
    data_rate_kbps: f64,
}

struct SimState {
    sats: Vec<SatState>,
    stations: Vec<StationState>,
    elapsed_s: f64,
    next_pass_id: u32,
    next_alert_id: u32,
    last_alert_s: f64,
    // Track which (station, sat) combos are currently in a pass
    active_passes: Vec<(usize, usize, u32)>, // (station_idx, sat_idx, pass_id)
}

impl SimState {
    fn new() -> Self {
        let sats = SATELLITES
            .iter()
            .enumerate()
            .map(|(i, def)| SatState {
                def_idx: i,
                angle_rad: def.initial_angle_deg.to_radians(),
                status: SatStatus::ACTIVE,
                frame_seq: 0,
            })
            .collect();

        let stations = STATIONS
            .iter()
            .enumerate()
            .map(|(i, _)| StationState {
                def_idx: i,
                tracking_sat: None,
                antenna_az_deg: 0.0,
                antenna_el_deg: 90.0,
                signal_db: -120.0,
                data_rate_kbps: 0.0,
            })
            .collect();

        Self {
            sats,
            stations,
            elapsed_s: 0.0,
            next_pass_id: 1,
            next_alert_id: 1,
            last_alert_s: 0.0,
            active_passes: Vec::new(),
        }
    }
}

// =============================================================================
// ORBITAL MECHANICS (simplified)
// =============================================================================

fn compute_sat_position(def: &SatDef, angle_rad: f64, elapsed_s: f64) -> (f64, f64) {
    let incl_rad = def.inclination_deg.to_radians();

    // Latitude: simplified as inclination * sin(angle)
    let lat_deg = def.inclination_deg * incl_rad.signum() * angle_rad.sin();
    // Clamp for GEO (inclination ~0)
    let lat_deg = if def.inclination_deg.abs() < 0.1 {
        0.0
    } else {
        lat_deg
    };

    // Longitude: initial + orbital progression - earth rotation
    let earth_rot_deg = EARTH_ROTATION_DEG_PER_SEC * elapsed_s;
    let lon_deg = def.initial_lon_deg + angle_rad.to_degrees() - earth_rot_deg;
    // Wrap to [-180, 180]
    let lon_deg = ((lon_deg + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;

    (lat_deg, lon_deg)
}

fn orbital_velocity_kmps(alt_km: f64) -> f64 {
    // v = sqrt(GM / r) where GM = 398600.4418 km^3/s^2
    let r = EARTH_RADIUS_KM + alt_km;
    (398600.4418 / r).sqrt()
}

/// Compute elevation angle from ground station to satellite (simplified).
/// Returns elevation in degrees. Positive means satellite is above horizon.
fn compute_elevation(
    station_lat: f64,
    station_lon: f64,
    sat_lat: f64,
    sat_lon: f64,
    sat_alt_km: f64,
) -> f64 {
    let dlat = (sat_lat - station_lat).to_radians();
    let dlon = (sat_lon - station_lon).to_radians();
    let slat = station_lat.to_radians();
    let clat = sat_lat.to_radians();

    // Haversine for angular distance
    let a = (dlat / 2.0).sin().powi(2) + slat.cos() * clat.cos() * (dlon / 2.0).sin().powi(2);
    let central_angle = 2.0 * a.sqrt().asin();

    if central_angle < 1e-10 {
        return 90.0; // directly overhead
    }

    let r_e = EARTH_RADIUS_KM;
    let r_s = r_e + sat_alt_km;

    // Elevation above the local horizon, negative when the satellite is below
    // it. The law of sines gives only the absolute value, since cos is even,
    // which made a satellite overhead report about 0 degrees and one below the
    // horizon report 60: every station then "saw" every satellite from the
    // first tick, and the pass logic never changed state again.
    let el_rad = (central_angle.cos() - r_e / r_s).atan2(central_angle.sin());

    el_rad.to_degrees()
}

/// Compute azimuth from station to satellite (degrees, 0=North, clockwise)
fn compute_azimuth(station_lat: f64, station_lon: f64, sat_lat: f64, sat_lon: f64) -> f64 {
    let lat1 = station_lat.to_radians();
    let lat2 = sat_lat.to_radians();
    let dlon = (sat_lon - station_lon).to_radians();

    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();

    let az = y.atan2(x).to_degrees();
    (az + 360.0) % 360.0
}

// =============================================================================
// TIMESTAMP HELPER
// =============================================================================

fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// =============================================================================
// DDS WRITERS
// =============================================================================

struct DdsWriters {
    position: hdds::DataWriter<SatellitePosition>,
    station: hdds::DataWriter<GroundStation>,
    pass: hdds::DataWriter<PassPrediction>,
    telemetry: hdds::DataWriter<TelemetryFrame>,
    alert: hdds::DataWriter<OrbitalAlert>,
}

impl DdsWriters {
    fn new(participant: &std::sync::Arc<Participant>) -> Result<Self, Box<dyn std::error::Error>> {
        let best_effort = QoS::best_effort();
        let reliable = QoS::reliable();

        Ok(Self {
            position: participant
                .topic::<SatellitePosition>(topics::SAT_POSITION)?
                .writer()
                .qos(best_effort.clone())
                .build()?,
            station: participant
                .topic::<GroundStation>(topics::GROUND_STATION)?
                .writer()
                .qos(best_effort.clone())
                .build()?,
            pass: participant
                .topic::<PassPrediction>(topics::PASS_PREDICTION)?
                .writer()
                .qos(reliable.clone())
                .build()?,
            telemetry: participant
                .topic::<TelemetryFrame>(topics::TELEMETRY_FRAME)?
                .writer()
                .qos(best_effort)
                .build()?,
            alert: participant.topic::<OrbitalAlert>(topics::ORBITAL_ALERT)?
                .writer()
                .qos(reliable)
                .build()?,
        })
    }
}

// =============================================================================
// MAIN
// =============================================================================

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    println!("============================================================");
    println!("  HDDS Satellite Ground Station Simulator");
    println!("  {} satellites, {} ground stations", SATELLITES.len(), STATIONS.len());
    println!("============================================================");
    println!();

    // Create HDDS DDS participant
    let participant = Participant::builder("sat-sim")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS participant created on domain 0 (UDP multicast)");

    let writers = DdsWriters::new(&participant)?;
    info!("Created DDS writers for 5 topics");

    // Print satellite info
    for def in SATELLITES {
        info!(
            "  SAT {} ({}) inc={:.1} alt={:.0}km period={:.0}min",
            def.norad_id, def.name, def.inclination_deg, def.alt_km, def.period_min
        );
    }
    for def in STATIONS {
        info!(
            "  GS  {} ({}) lat={:.1} lon={:.1}",
            def.id, def.name, def.lat_deg, def.lon_deg
        );
    }

    // Simulation loop at 1 Hz
    let mut state = SimState::new();
    let mut rng = rand::thread_rng();
    let tick = Duration::from_secs(1);

    info!("Simulation starting at 1 Hz...");
    println!();

    loop {
        let loop_start = Instant::now();
        state.elapsed_s += 1.0;
        let ts = now_ns();

        // =====================================================================
        // Update satellite positions
        // =====================================================================
        for sat in &mut state.sats {
            let def = &SATELLITES[sat.def_idx];
            let omega = 2.0 * PI / (def.period_min * 60.0);
            sat.angle_rad += omega; // 1 second step

            // Occasional status changes
            if rng.gen::<f64>() < 0.001 {
                sat.status = if rng.gen::<f64>() < 0.5 {
                    SatStatus::ECLIPSE
                } else {
                    SatStatus::ACTIVE
                };
            }
        }

        // Publish satellite positions
        for sat in &state.sats {
            let def = &SATELLITES[sat.def_idx];
            let (lat, lon) = compute_sat_position(def, sat.angle_rad, state.elapsed_s);
            let vel = orbital_velocity_kmps(def.alt_km);

            let pos = SatellitePosition {
                norad_id: def.norad_id.to_string(),
                name: def.name.to_string(),
                lat_deg: lat,
                lon_deg: lon,
                alt_km: def.alt_km,
                velocity_kmps: vel,
                inclination_deg: def.inclination_deg,
                period_min: def.period_min,
                status: sat.status,
                timestamp_ns: ts,
            };

            if let Err(e) = writers.position.write(&pos) {
                tracing::warn!("Failed to write position for {}: {}", def.name, e);
            }
        }

        // =====================================================================
        // Ground station tracking logic
        // =====================================================================

        // For each station, find the best satellite to track
        for st in &mut state.stations {
            let sdef = &STATIONS[st.def_idx];
            let mut best_sat: Option<(usize, f64, f64)> = None; // (idx, elevation, azimuth)

            for (si, sat) in state.sats.iter().enumerate() {
                let satdef = &SATELLITES[sat.def_idx];
                let (slat, slon) =
                    compute_sat_position(satdef, sat.angle_rad, state.elapsed_s);

                let el =
                    compute_elevation(sdef.lat_deg, sdef.lon_deg, slat, slon, satdef.alt_km);

                if el > ELEVATION_THRESHOLD_DEG {
                    if best_sat.is_none() || el > best_sat.unwrap().1 {
                        let az = compute_azimuth(sdef.lat_deg, sdef.lon_deg, slat, slon);
                        best_sat = Some((si, el, az));
                    }
                }
            }

            if let Some((sat_idx, el, az)) = best_sat {
                let satdef = &SATELLITES[state.sats[sat_idx].def_idx];

                // Signal strength: stronger at higher elevation, weaker for distant sats
                let dist_factor = (EARTH_RADIUS_KM + satdef.alt_km) / EARTH_RADIUS_KM;
                let el_factor = el / 90.0;
                let base_signal = -60.0 - 20.0 * dist_factor.log10();
                let signal = base_signal + 10.0 * el_factor + rng.gen_range(-2.0..2.0);

                // Data rate based on signal
                let data_rate = if signal > -80.0 {
                    256.0 + rng.gen_range(-10.0..10.0)
                } else if signal > -100.0 {
                    128.0 + rng.gen_range(-5.0..5.0)
                } else {
                    64.0 + rng.gen_range(-3.0..3.0)
                };

                st.tracking_sat = Some(sat_idx);
                st.antenna_az_deg = az;
                st.antenna_el_deg = el;
                st.signal_db = signal;
                st.data_rate_kbps = data_rate;
            } else {
                st.tracking_sat = None;
                st.antenna_az_deg = 0.0;
                st.antenna_el_deg = 90.0; // parked at zenith
                st.signal_db = -120.0;
                st.data_rate_kbps = 0.0;
            }
        }

        // Publish ground station status
        for st in &state.stations {
            let sdef = &STATIONS[st.def_idx];
            let mode = if st.tracking_sat.is_some() {
                StationMode::TRACKING
            } else {
                StationMode::IDLE
            };

            let tracking_name = st
                .tracking_sat
                .map(|idx| SATELLITES[state.sats[idx].def_idx].name.to_string())
                .unwrap_or_default();

            let gs = GroundStation {
                station_id: sdef.id.to_string(),
                name: sdef.name.to_string(),
                lat_deg: sdef.lat_deg,
                lon_deg: sdef.lon_deg,
                alt_m: sdef.alt_m,
                mode,
                antenna_az_deg: st.antenna_az_deg,
                antenna_el_deg: st.antenna_el_deg,
                tracking_sat: tracking_name,
                signal_db: st.signal_db,
                data_rate_kbps: st.data_rate_kbps,
                timestamp_ns: ts,
            };

            if let Err(e) = writers.station.write(&gs) {
                tracing::warn!("Failed to write station {}: {}", sdef.id, e);
            }
        }

        // =====================================================================
        // Pass prediction (AOS / LOS events)
        // =====================================================================

        // Check for new passes (station starts seeing a sat) and ended passes
        let mut new_passes: Vec<(usize, usize)> = Vec::new();
        let mut ended_passes: Vec<(usize, usize, u32)> = Vec::new();

        for (si, st) in state.stations.iter().enumerate() {
            for (sati, sat) in state.sats.iter().enumerate() {
                let satdef = &SATELLITES[sat.def_idx];
                let sdef = &STATIONS[st.def_idx];
                let (slat, slon) =
                    compute_sat_position(satdef, sat.angle_rad, state.elapsed_s);
                let el =
                    compute_elevation(sdef.lat_deg, sdef.lon_deg, slat, slon, satdef.alt_km);

                let is_active = state
                    .active_passes
                    .iter()
                    .any(|&(s, sa, _)| s == si && sa == sati);

                if el > ELEVATION_THRESHOLD_DEG && !is_active {
                    new_passes.push((si, sati));
                } else if el <= ELEVATION_THRESHOLD_DEG && is_active {
                    if let Some(&(_, _, pid)) = state
                        .active_passes
                        .iter()
                        .find(|&&(s, sa, _)| s == si && sa == sati)
                    {
                        ended_passes.push((si, sati, pid));
                    }
                }
            }
        }

        // Publish AOS events
        for (si, sati) in &new_passes {
            let pid = state.next_pass_id;
            state.next_pass_id += 1;
            state.active_passes.push((*si, *sati, pid));

            let sdef = &STATIONS[*si];
            let satdef = &SATELLITES[state.sats[*sati].def_idx];
            let (slat, slon) = compute_sat_position(
                satdef,
                state.sats[*sati].angle_rad,
                state.elapsed_s,
            );
            let el = compute_elevation(sdef.lat_deg, sdef.lon_deg, slat, slon, satdef.alt_km);
            let az = compute_azimuth(sdef.lat_deg, sdef.lon_deg, slat, slon);

            let pass = PassPrediction {
                pass_id: pid,
                norad_id: satdef.norad_id.to_string(),
                station_id: sdef.id.to_string(),
                phase: PassPhase::AOS,
                max_elevation_deg: el,
                aos_az_deg: az,
                los_az_deg: 0.0,
                aos_epoch_ms: now_ms(),
                los_epoch_ms: 0,
                duration_s: 0,
                timestamp_ns: ts,
            };

            if let Err(e) = writers.pass.write(&pass) {
                tracing::warn!("Failed to write AOS pass: {}", e);
            }

            info!(
                "AOS: {} sees {} (el={:.1} az={:.0})",
                sdef.name, satdef.name, el, az
            );
        }

        // Publish LOS events
        for (si, sati, pid) in &ended_passes {
            state
                .active_passes
                .retain(|&(s, sa, _)| !(s == *si && sa == *sati));

            let sdef = &STATIONS[*si];
            let satdef = &SATELLITES[state.sats[*sati].def_idx];

            let pass = PassPrediction {
                pass_id: *pid,
                norad_id: satdef.norad_id.to_string(),
                station_id: sdef.id.to_string(),
                phase: PassPhase::LOS,
                max_elevation_deg: 0.0,
                aos_az_deg: 0.0,
                los_az_deg: 0.0,
                aos_epoch_ms: 0,
                los_epoch_ms: now_ms(),
                duration_s: 0,
                timestamp_ns: ts,
            };

            if let Err(e) = writers.pass.write(&pass) {
                tracing::warn!("Failed to write LOS pass: {}", e);
            }

            info!("LOS: {} lost {}", sdef.name, satdef.name);
        }

        // =====================================================================
        // Telemetry frames (during active passes)
        // =====================================================================

        for &(si, sati, _pid) in &state.active_passes {
            let sat = &mut state.sats[sati];
            let satdef = &SATELLITES[sat.def_idx];
            let sdef = &STATIONS[si];

            sat.frame_seq += 1;

            let frame = TelemetryFrame {
                norad_id: satdef.norad_id.to_string(),
                station_id: sdef.id.to_string(),
                frame_seq: sat.frame_seq,
                battery_v: 28.0 + rng.gen_range(-2.0..2.0),
                solar_w: if sat.status == SatStatus::ECLIPSE {
                    0.0
                } else {
                    350.0 + rng.gen_range(-50.0..50.0)
                },
                cpu_temp_c: 35.0 + rng.gen_range(-5.0..15.0),
                signal_strength_db: state.stations[si].signal_db + rng.gen_range(-1.0..1.0),
                bit_errors: rng.gen_range(0..5),
                bytes_received: rng.gen_range(1000..8000),
                timestamp_ns: ts,
            };

            if let Err(e) = writers.telemetry.write(&frame) {
                tracing::warn!("Failed to write telemetry for {}: {}", satdef.name, e);
            }
        }

        // =====================================================================
        // Occasional orbital alerts (conjunction warnings)
        // =====================================================================

        if state.elapsed_s - state.last_alert_s > 30.0 && rng.gen::<f64>() < 0.05 {
            let sat_idx = rng.gen_range(0..SATELLITES.len());
            let satdef = &SATELLITES[sat_idx];

            let (alert_type, desc, miss_km) = if rng.gen::<f64>() < 0.7 {
                (
                    "CONJUNCTION",
                    format!(
                        "Close approach predicted for {} with debris object",
                        satdef.name
                    ),
                    rng.gen_range(0.5..50.0),
                )
            } else {
                (
                    "ORBIT_DECAY",
                    format!(
                        "Altitude decay detected for {}, {:.1}m/day",
                        satdef.name,
                        rng.gen_range(1.0..10.0)
                    ),
                    0.0,
                )
            };

            let alert = OrbitalAlert {
                alert_id: state.next_alert_id,
                norad_id: satdef.norad_id.to_string(),
                alert_type: alert_type.to_string(),
                description: desc,
                miss_distance_km: miss_km,
                timestamp_ns: ts,
            };

            state.next_alert_id += 1;
            state.last_alert_s = state.elapsed_s;

            if let Err(e) = writers.alert.write(&alert) {
                tracing::warn!("Failed to write orbital alert: {}", e);
            }

            info!(
                "ALERT [{}]: {} (miss={:.1}km)",
                alert_type, satdef.name, miss_km
            );
        }

        // =====================================================================
        // Log summary every 10 seconds
        // =====================================================================

        if state.elapsed_s as u64 % 10 == 0 {
            let active = state.active_passes.len();
            let tracking: Vec<String> = state
                .stations
                .iter()
                .map(|st| {
                    let sdef = &STATIONS[st.def_idx];
                    match st.tracking_sat {
                        Some(idx) => format!(
                            "{}->{}",
                            sdef.id,
                            SATELLITES[state.sats[idx].def_idx].name
                        ),
                        None => format!("{}: idle", sdef.id),
                    }
                })
                .collect();
            info!(
                "t={:.0}s  passes={}  [{}]",
                state.elapsed_s,
                active,
                tracking.join(", ")
            );
        }

        // Sleep remainder of tick
        let elapsed = loop_start.elapsed();
        if elapsed < tick {
            std::thread::sleep(tick - elapsed);
        }
    }
}
