//! Railway Network Simulator
//!
//! Simulates a railway network with:
//! - Trains moving along track segments
//! - Signals controlling train flow
//! - Switches for track branching
//! - Stations for stops

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use railway_types::railway::*;
use rand::Rng;

/// Topic names for DDS
mod topics {
    pub const TRAIN: &str = "railway/train";
    pub const SIGNAL: &str = "railway/signal";
    pub const TRACK_SEGMENT: &str = "railway/track_segment";
    pub const STATION: &str = "railway/station";
    pub const NETWORK_STATE: &str = "railway/network_state";
}

/// Get current timestamp in microseconds
fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros() as u64
}
const TICK_RATE_HZ: u64 = 30;

/// World dimensions (pixels/units for display)
const WORLD_WIDTH: f32 = 1200.0;
const WORLD_HEIGHT: f32 = 800.0;

/// Train physics
const TRAIN_ACCEL: f32 = 20.0;      // units/s^2
const TRAIN_BRAKE: f32 = 40.0;      // units/s^2
const TRAIN_MAX_SPEED: f32 = 100.0; // units/s
const STATION_WAIT_TIME: f32 = 3.0; // seconds

// ============================================================================
// Simulation state
// ============================================================================

/// Track layout builder
struct TrackLayout {
    segments: Vec<TrackSegmentState>,
    stations: Vec<StationState>,
    signals: Vec<SignalSim>,
}

/// Internal track segment with computed data
#[derive(Clone)]
struct TrackSegmentState {
    segment: TrackSegment,
}

/// Internal station state
struct StationState {
    station: Station,
}

/// Internal signal state
struct SignalSim {
    signal: Signal,
    auto_timer: f32,
}

/// Train simulation state
struct TrainSim {
    train: Train,
    wait_timer: f32,
    route: Vec<u32>,  // List of segment IDs to follow
    route_idx: usize,
}

impl TrainSim {
    fn new(id: u32, name: &str, color: u32, segment: u32, route: Vec<u32>) -> Self {
        Self {
            train: Train {
                train_id: id,
                name: name.to_string(),
                color,
                current_segment: segment,
                position: 0.0,
                speed: 0.0,
                direction: 1,
                state: TrainState::TRAIN_STOPPED,
                next_station: 0,
                delay_seconds: 0.0,
                timestamp_us: 0,
            },
            wait_timer: 0.0,
            route,
            route_idx: 0,
        }
    }

}

impl TrackLayout {
    fn new() -> Self {
        Self {
            segments: Vec::new(),
            stations: Vec::new(),
            signals: Vec::new(),
        }
    }

    /// Create a simple oval track layout with stations
    fn create_oval_layout() -> Self {
        let mut layout = Self::new();

        // Oval track with 8 segments
        // Top straight
        layout.add_segment(1, TrackType::TRACK_STRAIGHT, 200.0, 200.0, 500.0, 200.0, 2, 8);
        layout.add_segment(2, TrackType::TRACK_STRAIGHT, 500.0, 200.0, 800.0, 200.0, 3, 1);

        // Right curve
        layout.add_segment(3, TrackType::TRACK_CURVE_RIGHT, 800.0, 200.0, 1000.0, 400.0, 4, 2);

        // Right straight
        layout.add_segment(4, TrackType::TRACK_STRAIGHT, 1000.0, 400.0, 1000.0, 600.0, 5, 3);

        // Bottom curve
        layout.add_segment(5, TrackType::TRACK_CURVE_RIGHT, 1000.0, 600.0, 800.0, 700.0, 6, 4);

        // Bottom straight
        layout.add_segment(6, TrackType::TRACK_STRAIGHT, 800.0, 700.0, 400.0, 700.0, 7, 5);

        // Left curve
        layout.add_segment(7, TrackType::TRACK_CURVE_RIGHT, 400.0, 700.0, 200.0, 500.0, 8, 6);

        // Left straight connecting back
        layout.add_segment(8, TrackType::TRACK_STRAIGHT, 200.0, 500.0, 200.0, 200.0, 1, 7);

        // Stations
        layout.add_station(1, "Central", 1, 0.5, 350.0, 160.0, 2);
        layout.add_station(2, "East Side", 4, 0.5, 1040.0, 500.0, 1);
        layout.add_station(3, "South", 6, 0.5, 600.0, 740.0, 2);
        layout.add_station(4, "West End", 8, 0.5, 160.0, 350.0, 1);

        // Signals at key points
        layout.add_signal(1, 1, 0.8);
        layout.add_signal(2, 3, 0.5);
        layout.add_signal(3, 5, 0.5);
        layout.add_signal(4, 7, 0.5);

        layout
    }

    fn add_segment(&mut self, id: u32, track_type: TrackType, x1: f32, y1: f32, x2: f32, y2: f32, next: u32, prev: u32) {
        let length = ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt();
        let segment = TrackSegment {
            segment_id: id,
            track_type,
            x1, y1, x2, y2,
            next_segment: next,
            prev_segment: prev,
            length,
            speed_limit: TRAIN_MAX_SPEED,
            has_platform: false,
        };
        self.segments.push(TrackSegmentState { segment });
    }

    fn add_station(&mut self, id: u32, name: &str, segment_id: u32, position: f32, x: f32, y: f32, platforms: u8) {
        // Mark segment as having platform
        if let Some(seg) = self.segments.iter_mut().find(|s| s.segment.segment_id == segment_id) {
            seg.segment.has_platform = true;
        }

        let station = Station {
            station_id: id,
            name: name.to_string(),
            segment_id,
            position,
            x, y,
            platform_count: platforms,
        };
        self.stations.push(StationState { station });
    }

    fn add_signal(&mut self, id: u32, segment_id: u32, position: f32) {
        let signal = Signal {
            signal_id: id,
            segment_id,
            position,
            state: SignalState::SIGNAL_GREEN,
            timestamp_us: 0,
        };
        self.signals.push(SignalSim { signal, auto_timer: 0.0 });
    }

    fn get_segment(&self, id: u32) -> Option<&TrackSegment> {
        self.segments.iter()
            .find(|s| s.segment.segment_id == id)
            .map(|s| &s.segment)
    }

    /// Check if there's a station at current position
    fn station_at(&self, segment_id: u32, position: f32) -> Option<u32> {
        for st in &self.stations {
            if st.station.segment_id == segment_id {
                let diff = (st.station.position - position).abs();
                if diff < 0.1 {
                    return Some(st.station.station_id);
                }
            }
        }
        None
    }

    /// Check if signal allows passage
    fn signal_allows(&self, segment_id: u32, position: f32) -> bool {
        for sig in &self.signals {
            if sig.signal.segment_id == segment_id && sig.signal.position <= position + 0.1 {
                return sig.signal.state == SignalState::SIGNAL_GREEN;
            }
        }
        true // No signal = allowed
    }



}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    log::info!("Railway Network Simulator starting...");

    // Create DDS participant
    let participant = Participant::builder("railway_sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    // Create writers
    let train_writer = participant.topic::<Train>(topics::TRAIN)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let signal_writer = participant.topic::<Signal>(topics::SIGNAL)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let segment_writer = participant.topic::<TrackSegment>(topics::TRACK_SEGMENT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let station_writer = participant.topic::<Station>(topics::STATION)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let network_writer = participant.topic::<NetworkState>(topics::NETWORK_STATE)?
        .writer()
        .qos(qos)
        .build()?;

    // Create track layout
    let mut layout = TrackLayout::create_oval_layout();

    // Publish static track data
    for seg in &layout.segments {
        let _ = segment_writer.write(&seg.segment);
    }
    for st in &layout.stations {
        let _ = station_writer.write(&st.station);
    }

    // Create trains
    let mut trains = vec![
        TrainSim::new(1, "Express A", 0xFF4444, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]),
        TrainSim::new(2, "Local B", 0x44FF44, 4, vec![4, 5, 6, 7, 8, 1, 2, 3]),
        TrainSim::new(3, "Night C", 0x4444FF, 6, vec![6, 7, 8, 1, 2, 3, 4, 5]),
    ];

    // Stagger initial positions
    trains[0].train.position = 0.0;
    trains[1].train.position = 0.0;
    trains[2].train.position = 0.0;

    let tick_duration = std::time::Duration::from_micros(1_000_000 / TICK_RATE_HZ);
    let mut last_tick = std::time::Instant::now();
    let mut rng = rand::thread_rng();

    log::info!("Simulation running with {} trains on {} segments", trains.len(), layout.segments.len());

    loop {
        let now = std::time::Instant::now();
        let dt = (now - last_tick).as_secs_f32();
        last_tick = now;

        // Update signals (simple auto cycling)
        for sig in &mut layout.signals {
            sig.auto_timer += dt;
            if sig.auto_timer > 5.0 + rng.gen::<f32>() * 3.0 {
                sig.auto_timer = 0.0;
                sig.signal.state = match sig.signal.state {
                    SignalState::SIGNAL_GREEN => SignalState::SIGNAL_YELLOW,
                    SignalState::SIGNAL_YELLOW => SignalState::SIGNAL_RED,
                    SignalState::SIGNAL_RED => SignalState::SIGNAL_GREEN,
                };
                sig.signal.timestamp_us = timestamp_us();
                let _ = signal_writer.write(&sig.signal);
            }
        }

        // Update trains
        let mut trains_at_station = 0u8;
        let mut trains_running = 0u8;

        for train_sim in &mut trains {
            let train = &mut train_sim.train;

            match train.state {
                TrainState::TRAIN_AT_STATION => {
                    trains_at_station += 1;
                    train_sim.wait_timer -= dt;
                    if train_sim.wait_timer <= 0.0 {
                        train.state = TrainState::TRAIN_ACCELERATING;
                    }
                }

                TrainState::TRAIN_STOPPED => {
                    // Check if signal is now green
                    if layout.signal_allows(train.current_segment, train.position) {
                        train.state = TrainState::TRAIN_ACCELERATING;
                    }
                }

                TrainState::TRAIN_ACCELERATING => {
                    trains_running += 1;
                    train.speed += TRAIN_ACCEL * dt;
                    if train.speed >= TRAIN_MAX_SPEED * 0.9 {
                        train.state = TrainState::TRAIN_CRUISING;
                    }
                }

                TrainState::TRAIN_CRUISING => {
                    trains_running += 1;
                    // Check for upcoming station or signal
                    let stop_ahead = layout.station_at(train.current_segment, train.position + 0.2).is_some()
                        || !layout.signal_allows(train.current_segment, train.position + 0.15);

                    if stop_ahead {
                        train.state = TrainState::TRAIN_BRAKING;
                    }
                }

                TrainState::TRAIN_BRAKING => {
                    trains_running += 1;
                    train.speed -= TRAIN_BRAKE * dt;
                    if train.speed <= 0.0 {
                        train.speed = 0.0;

                        // Check if at station
                        if let Some(station_id) = layout.station_at(train.current_segment, train.position) {
                            train.state = TrainState::TRAIN_AT_STATION;
                            train.next_station = station_id;
                            train_sim.wait_timer = STATION_WAIT_TIME;
                        } else {
                            train.state = TrainState::TRAIN_STOPPED;
                        }
                    }
                }
            }

            // Move train along track
            if train.speed > 0.0 {
                if let Some(seg) = layout.get_segment(train.current_segment) {
                    let move_dist = train.speed * dt / seg.length;
                    train.position += move_dist * train.direction as f32;

                    // Handle segment transitions
                    if train.position >= 1.0 {
                        train.position = 0.0;
                        // Move to next segment in route
                        train_sim.route_idx = (train_sim.route_idx + 1) % train_sim.route.len();
                        train.current_segment = train_sim.route[train_sim.route_idx];
                    } else if train.position < 0.0 {
                        train.position = 1.0;
                        train_sim.route_idx = if train_sim.route_idx == 0 {
                            train_sim.route.len() - 1
                        } else {
                            train_sim.route_idx - 1
                        };
                        train.current_segment = train_sim.route[train_sim.route_idx];
                    }
                }
            }

            train.timestamp_us = timestamp_us();
            let _ = train_writer.write(train);
        }

        // Network state
        let network = NetworkState {
            train_count: trains.len() as u8,
            trains_running,
            trains_at_station,
            signals_red: layout.signals.iter().filter(|s| s.signal.state == SignalState::SIGNAL_RED).count() as u8,
            signals_green: layout.signals.iter().filter(|s| s.signal.state == SignalState::SIGNAL_GREEN).count() as u8,
            avg_delay_seconds: 0.0,
            timestamp_us: timestamp_us(),
        };
        let _ = network_writer.write(&network);

        tokio::time::sleep(tick_duration).await;
    }
}
