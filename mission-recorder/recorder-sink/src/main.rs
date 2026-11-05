// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! Mission Recorder - InfluxDB Sink
//!
//! Subscribes to DDS mission topics and writes events to InfluxDB Line Protocol
//! format (.lp file). Uses the hdds-influx-sink crate's LineProtocolWriter for
//! proper escaping and formatting.
//!
//! Output: `mission_recording.lp`

use hdds::{Participant, QoS, TransportMode};
use hdds_influx_sink::influx::{FieldValue, LineProtocolWriter};
use recorder_types::{topics, MissionEvent, MissionStatus};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{info, warn};

// =============================================================================
// CONFIGURATION
// =============================================================================

/// Recording duration -- same as the sim (5 minutes).
const RECORD_DURATION_S: f64 = 300.0;

/// Flush interval for writing to disk.
const FLUSH_INTERVAL: Duration = Duration::from_secs(2);

// =============================================================================
// LINE PROTOCOL HELPERS
// =============================================================================

/// Convert a MissionEvent to InfluxDB Line Protocol via the hdds-influx-sink writer.
fn event_to_line_protocol(writer: &mut LineProtocolWriter, evt: &MissionEvent) {
    let tags: Vec<(&str, &str)> = vec![
        ("source", evt.source.as_str()),
        ("type", evt.event_type.as_str()),
    ];

    let fields: Vec<(&str, FieldValue)> = vec![
        ("event_id", FieldValue::Integer(evt.event_id)),
        ("latitude", FieldValue::Float(evt.latitude)),
        ("longitude", FieldValue::Float(evt.longitude)),
        ("altitude_m", FieldValue::Float(evt.altitude_m)),
        ("heading_deg", FieldValue::Float(evt.heading_deg as f64)),
        ("speed_knots", FieldValue::Float(evt.speed_knots as f64)),
        ("payload", FieldValue::String(evt.payload.clone())),
    ];

    writer.write_point(
        "mission_events",
        &tags,
        &fields,
        evt.timestamp_ns as u64,
    );
}

/// Convert a MissionStatus to InfluxDB Line Protocol.
fn status_to_line_protocol(writer: &mut LineProtocolWriter, st: &MissionStatus) {
    let tags: Vec<(&str, &str)> = vec![
        ("mission_id", st.mission_id.as_str()),
        ("phase", st.phase.as_str()),
    ];

    let fields: Vec<(&str, FieldValue)> = vec![
        ("elapsed_seconds", FieldValue::Float(st.elapsed_seconds)),
        ("events_recorded", FieldValue::Integer(st.events_recorded)),
        ("participants_active", FieldValue::Integer(st.participants_active as i64)),
    ];

    writer.write_point(
        "mission_status",
        &tags,
        &fields,
        st.timestamp_ns as u64,
    );
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
    info!("  HDDS Mission Recorder - InfluxDB Sink");
    info!("============================================");

    // -------------------------------------------------------------------------
    // DDS setup
    // -------------------------------------------------------------------------
    let participant = Participant::builder("mission_sink")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();
    let event_reader = participant.topic::<MissionEvent>(topics::MISSION_EVENT)?
        .reader()
        .qos(qos.clone())
        .build()?;
    let status_reader = participant.topic::<MissionStatus>(topics::MISSION_STATUS)?
        .reader()
        .qos(qos)
        .build()?;

    info!("DDS participant created, readers ready");

    // -------------------------------------------------------------------------
    // Output file
    // -------------------------------------------------------------------------
    let output_path = PathBuf::from("mission_recording.lp");
    let mut output_file = File::create(&output_path)?;
    info!("Recording to: {}", output_path.display());

    // -------------------------------------------------------------------------
    // Recording loop
    // -------------------------------------------------------------------------
    let mut lp_writer = LineProtocolWriter::new();
    let mut events_recorded: u64 = 0;
    let mut status_recorded: u64 = 0;
    let mut bytes_written: u64 = 0;

    let start = Instant::now();
    let mut last_flush = Instant::now();
    let mut last_stats = Instant::now();

    info!("Waiting for mission data...");

    loop {
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed >= RECORD_DURATION_S {
            info!("Recording duration reached, finalizing...");
            break;
        }

        // Poll for mission events
        loop {
            match event_reader.take() {
                Ok(Some(evt)) => {
                    event_to_line_protocol(&mut lp_writer, &evt);
                    events_recorded += 1;
                }
                Ok(None) => break,
                Err(e) => {
                    warn!("event read error: {}", e);
                    break;
                }
            }
        }

        // Poll for mission status
        loop {
            match status_reader.take() {
                Ok(Some(st)) => {
                    status_to_line_protocol(&mut lp_writer, &st);
                    status_recorded += 1;
                }
                Ok(None) => break,
                Err(e) => {
                    warn!("status read error: {}", e);
                    break;
                }
            }
        }

        // Periodic flush to disk
        if last_flush.elapsed() >= FLUSH_INTERVAL {
            let lines = lp_writer.flush();
            if !lines.is_empty() {
                for line in &lines {
                    let line_bytes = line.as_bytes();
                    output_file.write_all(line_bytes)?;
                    output_file.write_all(b"\n")?;
                    bytes_written += line_bytes.len() as u64 + 1;
                }
                output_file.flush()?;
            }
            last_flush = Instant::now();
        }

        // Stats printout every 10 seconds
        if last_stats.elapsed() >= Duration::from_secs(10) {
            let rate = events_recorded as f64 / elapsed;
            info!(
                "[{:>6.1}s] events={} status={} size={:.1}KB rate={:.1} evt/s",
                elapsed,
                events_recorded,
                status_recorded,
                bytes_written as f64 / 1024.0,
                rate,
            );
            last_stats = Instant::now();
        }

        // Avoid busy-loop
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // Final flush
    let lines = lp_writer.flush();
    for line in &lines {
        output_file.write_all(line.as_bytes())?;
        output_file.write_all(b"\n")?;
        bytes_written += line.as_bytes().len() as u64 + 1;
    }
    output_file.flush()?;

    info!("============================================");
    info!("  Recording Complete");
    info!("============================================");
    info!("  Events recorded:  {}", events_recorded);
    info!("  Status recorded:  {}", status_recorded);
    info!("  File size:        {:.1} KB", bytes_written as f64 / 1024.0);
    info!("  Output:           {}", output_path.display());
    info!("============================================");

    Ok(())
}
