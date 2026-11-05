// CONCEPT DEMO - Simulated data for showcase purposes
//
//! Cockpit Sensor Simulator
//!
//! Publishes 4 simulated sensors at 10Hz and a QoS snapshot at 1Hz.
//! Demonstrates HDDS hot-reload by watching a QoS profile YAML file
//! and applying mutable QoS changes at runtime.

use anyhow::Result;
use cockpit_types::*;
use hdds::dds::qos::hot_reload::QosFileWatcher;
use hdds::dds::qos::profiles::QosProfileRegistry;
use hdds::{Participant, QoS, TransportMode};
use log::*;
use rand::Rng;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Sensor definitions: (id, base_value, amplitude, noise_scale)
const SENSORS: &[(&str, f64, f64, f64)] = &[
    ("airspeed", 250.0, 20.0, 2.0),
    ("altitude", 10000.0, 500.0, 10.0),
    ("heading", 180.0, 15.0, 1.0),
    ("temperature", 22.0, 5.0, 0.5),
];

/// Interval between sensor publishes (10 Hz)
const SENSOR_INTERVAL: Duration = Duration::from_millis(100);

/// Interval between QoS snapshot publishes (1 Hz)
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

/// Default QoS profile path (relative to working directory)
fn default_qos_path() -> PathBuf {
    // Try relative path first, then absolute
    let relative = PathBuf::from("qos-profiles/default.yaml");
    if relative.exists() {
        return relative;
    }
    PathBuf::from("/projects/public/demos/hot-reload-cockpit/qos-profiles/default.yaml")
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("==========================================");
    info!("  HDDS QoS Hot-Reload Cockpit - Simulator");
    info!("  4 sensors @ 10Hz, QoS snapshot @ 1Hz");
    info!("==========================================");

    // -- Load QoS profile registry --
    let qos_path = default_qos_path();
    let registry = Arc::new(QosProfileRegistry::new());

    let active_qos = if qos_path.exists() {
        match registry.load_from_yaml(&qos_path) {
            Ok(count) => {
                info!("Loaded {} QoS profile(s) from {}", count, qos_path.display());
                registry.get("default").unwrap_or_else(QoS::default)
            }
            Err(e) => {
                warn!("Failed to load QoS profile: {}. Using defaults.", e);
                QoS::default()
            }
        }
    } else {
        info!("No QoS profile file found at {}. Using defaults.", qos_path.display());
        QoS::default()
    };

    // -- Start hot-reload watcher --
    #[allow(unused_variables)]
    let watcher: Option<QosFileWatcher> = if qos_path.exists() {
        let mut w = QosFileWatcher::new(
            qos_path.clone(),
            registry.clone(),
            Duration::from_secs(2),
        );
        match w.start() {
            Ok(()) => {
                info!("QoS hot-reload watcher started (polling every 2s)");
                info!("  Watching: {}", qos_path.display());
            }
            Err(e) => {
                warn!("Failed to start QoS watcher: {}", e);
            }
        }
        Some(w)
    } else {
        None
    };

    // -- Create DDS participant --
    let participant = Participant::builder("cockpit-sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS participant created");

    // Create writers with the loaded QoS
    let sensor_writer =
        participant.topic::<SensorFeed>(topics::SENSOR_FEED)?
            .writer()
            .qos(active_qos.clone())
            .build()?;
    let snapshot_writer =
        participant.topic::<QosSnapshot>(topics::QOS_SNAPSHOT)?
            .writer()
            .qos(active_qos.clone())
            .build()?;
    let metrics_writer =
        participant.topic::<MetricsDelta>(topics::METRICS_DELTA)?
            .writer()
            .qos(active_qos.clone())
            .build()?;

    info!("Writers created on topics:");
    info!("  {} (SensorFeed)", topics::SENSOR_FEED);
    info!("  {} (QosSnapshot)", topics::QOS_SNAPSHOT);
    info!("  {} (MetricsDelta)", topics::METRICS_DELTA);

    // -- Main simulation loop --
    let mut rng = rand::thread_rng();
    let mut tick: u64 = 0;
    let mut last_snapshot = Instant::now();
    let mut last_sensor = Instant::now();
    let mut last_reload_check = Instant::now();

    // Simulated metrics tracking
    let mut latency_us: f64 = 150.0;
    let mut missed_deadlines: i32 = 0;
    let mut throughput_mps: f64 = 40.0;

    info!("Starting simulation loop...");

    loop {
        let now = Instant::now();

        // -- Publish sensor data at 10Hz --
        if now.duration_since(last_sensor) >= SENSOR_INTERVAL {
            last_sensor = now;
            tick += 1;

            let t = tick as f64 * 0.1; // Time in seconds

            for &(id, base, amplitude, noise) in SENSORS {
                let value = base + amplitude * (t * 0.3).sin() + rng.gen_range(-noise..noise);
                let quality = (0.85 + 0.15 * (t * 0.1).cos()).clamp(0.0, 1.0);

                let feed = SensorFeed {
                    sensor_id: id.to_string(),
                    value,
                    quality,
                    timestamp_ns: timestamp_ns(),
                };

                if let Err(e) = sensor_writer.write(&feed) {
                    warn!("Failed to write sensor {}: {}", id, e);
                }
            }

            // Simulate metrics evolution
            latency_us += rng.gen_range(-5.0..5.0);
            latency_us = latency_us.clamp(50.0, 500.0);
            if rng.gen_bool(0.02) {
                missed_deadlines += 1;
            }
            throughput_mps = 40.0 + rng.gen_range(-2.0..2.0);
        }

        // -- Publish QoS snapshot at 1Hz --
        if now.duration_since(last_snapshot) >= SNAPSHOT_INTERVAL {
            last_snapshot = now;

            // Get current QoS from registry (may have been hot-reloaded)
            let current_qos = registry.get("default").unwrap_or_else(QoS::default);

            let snapshot = QosSnapshot {
                entity_name: "cockpit-sim/sensor_writer".to_string(),
                reliability: format!("{:?}", current_qos.reliability),
                deadline_ms: current_qos.deadline.period.as_millis() as i32,
                history_depth: match current_qos.history {
                    hdds::dds::History::KeepLast(d) => d as i32,
                    hdds::dds::History::KeepAll => -1,
                },
                partition: format!("{:?}", current_qos.partition.names),
                transport_priority: current_qos.transport_priority.value,
                latency_budget_ms: current_qos.latency_budget.duration.as_millis() as i32,
                lifespan_ms: current_qos.lifespan.duration.as_millis() as i32,
                time_based_filter_ms: current_qos
                    .time_based_filter
                    .minimum_separation
                    .as_millis() as i32,
                captured_ns: timestamp_ns(),
            };

            if let Err(e) = snapshot_writer.write(&snapshot) {
                warn!("Failed to write QoS snapshot: {}", e);
            }
        }

        // -- Check for hot-reload changes every 3s --
        if now.duration_since(last_reload_check) >= Duration::from_secs(3) {
            last_reload_check = now;

            if let Some(ref w) = watcher {
                if let Some(result) = w.last_reload_result() {
                    if result.has_changes() {
                        info!(
                            "QoS hot-reload detected: {} updated, {} added, {} removed",
                            result.profiles_updated.len(),
                            result.profiles_added.len(),
                            result.profiles_removed.len()
                        );

                        // Publish metrics delta showing impact
                        let new_latency = latency_us * 0.8; // Simulate improvement
                        let delta = MetricsDelta {
                            entity_name: "cockpit-sim/sensor_writer".to_string(),
                            latency_before_us: latency_us,
                            latency_after_us: new_latency,
                            missed_deadlines_before: missed_deadlines,
                            missed_deadlines_after: 0,
                            throughput_before_mps: throughput_mps,
                            throughput_after_mps: throughput_mps * 1.1,
                            timestamp_ns: timestamp_ns(),
                        };

                        if let Err(e) = metrics_writer.write(&delta) {
                            warn!("Failed to write metrics delta: {}", e);
                        }

                        // Reset simulated metrics
                        latency_us = new_latency;
                        missed_deadlines = 0;
                    }
                }
            }
        }

        std::thread::sleep(Duration::from_millis(10));
    }
}
