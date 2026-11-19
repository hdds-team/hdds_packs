// CONCEPT DEMO - Simulated data for showcase purposes
//
// UDS Fleet Diagnostic Monitor
//
// Subscribes to all diagnostic DDS topics published by the bridge
// and displays a real-time view of fleet ECU status, latency,
// firmware transfers, and AI diagnostic alerts.
//
// This demonstrates how multiple monitoring tools can subscribe
// to the same DDS topics independently -- no broker, no config,
// just launch and it discovers the bridge automatically via SPDP.

use std::time::Duration;

use diag_types::*;
use hdds::{Participant, QoS, TransportMode};
use tracing::info;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    println!("============================================================");
    println!("  HDDS UDS Fleet Diagnostic Monitor");
    println!("  Subscribing to diagnostic DDS topics");
    println!("============================================================");
    println!();

    let participant = Participant::builder("diag-monitor")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS participant created on domain 0");

    let health_reader = participant.topic::<EcuHealth>("diag/ecu/health")?
        .reader()
        .qos(QoS::reliable().transient_local())
        .build()?;
    let latency_reader =
        participant.topic::<P2Latency>("diag/ecu/latency")?
            .reader()
            .qos(QoS::best_effort())
            .build()?;
    let transfer_reader =
        participant.topic::<TransferProgress>("diag/ecu/transfer")?
            .reader()
            .qos(QoS::reliable())
            .build()?;
    let alert_reader = participant.topic::<DiagAlert>("diag/fleet/anomalies")?
        .reader()
        .qos(QoS::reliable().keep_all())
        .build()?;
    let fleet_reader =
        participant.topic::<FleetStats>("diag/fleet/stats")?
            .reader()
            .qos(QoS::best_effort())
            .build()?;
    let session_reader =
        participant.topic::<SessionChange>("diag/ecu/session")?
            .reader()
            .qos(QoS::reliable())
            .build()?;

    info!("6 readers created, listening for data...");
    println!();

    loop {
        if let Ok(Some(s)) = health_reader.take() {
            println!(
                "  [HEALTH] {} | {} {} | rx={} tx={} drop={} | bus={}% | req={} err={}",
                s.ecu_id,
                s.state.name(),
                s.session.name(),
                s.rx_total,
                s.tx_total,
                s.rx_dropped,
                s.bus_load_pct,
                s.requests_handled,
                s.server_errors,
            );
        }

        if let Ok(Some(s)) = latency_reader.take() {
            println!(
                "  [P2]     {} | last={}us min={}us max={}us avg={}us (n={})",
                s.ecu_id, s.last_p2_us, s.min_p2_us, s.max_p2_us, s.avg_p2_us, s.samples,
            );
        }

        if let Ok(Some(s)) = transfer_reader.take() {
            println!(
                "  [FLASH]  {} | {}% ({}/{}) retries={} | {}ms | {} bytes",
                s.ecu_id,
                s.progress_pct,
                s.blocks_sent,
                s.total_blocks,
                s.total_retries,
                s.elapsed_ms,
                s.bytes_transferred,
            );
        }

        if let Ok(Some(s)) = alert_reader.take() {
            let marker = match s.severity {
                AlertSeverity::WARNING => "!!",
                AlertSeverity::ALERT_ERROR => "!!!",
                AlertSeverity::CRITICAL => "!!!!",
                _ => "--",
            };
            println!(
                "  [ALERT {} {}] {} #{}: {} ({})",
                marker,
                s.severity.name(),
                s.ecu_id,
                s.alert_seq,
                s.summary,
                s.engine,
            );
        }

        if let Ok(Some(s)) = fleet_reader.take() {
            println!(
                "  [FLEET]  {} | online={} flash={} err={} | req={} | bus={:.0}%",
                s.bridge_id,
                s.ecus_online,
                s.ecus_flashing,
                s.ecus_error,
                s.total_requests,
                s.avg_bus_load_pct,
            );
        }

        if let Ok(Some(s)) = session_reader.take() {
            println!(
                "  [SESSION] {} | {} -> {}",
                s.ecu_id,
                s.previous_session.name(),
                s.new_session.name(),
            );
        }

        std::thread::sleep(Duration::from_millis(100));
    }
}
