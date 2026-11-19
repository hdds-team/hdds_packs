// CONCEPT DEMO - Simulated data for showcase purposes
//
// UDS Fleet Diagnostic Bridge
//
// Simulates a fleet of 4 automotive ECUs publishing diagnostic data
// over DDS topics via HDDS. Demonstrates how UDS (ISO 14229) diagnostic
// metrics can be distributed in real-time using DDS pub/sub.
//
// Topics published:
//   diag/ecu/health      RELIABLE+TRANSIENT_LOCAL  1 Hz   ECU health status
//   diag/ecu/latency     BEST_EFFORT               1 Hz   P2 latency metrics
//   diag/ecu/transfer    RELIABLE                   event  Firmware flash progress
//   diag/ecu/session     RELIABLE                   event  Session changes
//   diag/fleet/anomalies RELIABLE+KEEP_ALL          event  AI diagnostic alerts
//   diag/fleet/stats     BEST_EFFORT               1 Hz   Fleet aggregates
//
// This bridges the HUDS automotive diagnostic stack (CAN/UDS) with HDDS
// DDS middleware for distributed fleet monitoring.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use diag_types::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use tracing::info;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// ---------------------------------------------------------------------------
//  Simulated ECU state (mirrors HUDS uds-core/uds-server/uds-client types)
// ---------------------------------------------------------------------------

struct SimulatedEcu {
    id: String,
    state: EcuState,
    session: DiagSession,
    rx_total: u64,
    tx_total: u64,
    rx_dropped: u64,
    requests_handled: u64,
    responses_sent: u64,
    server_errors: u64,
    last_p2_us: u64,
    min_p2_us: u64,
    max_p2_us: u64,
    avg_p2_us: u64,
    latency_samples: u64,
    flashing: bool,
    blocks_sent: u32,
    total_blocks: u32,
    total_retries: u32,
    bytes_transferred: u64,
    flash_start_ms: u64,
    alert_seq: u32,
    bus_load_pct: u32,
}

impl SimulatedEcu {
    fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            state: EcuState::IDLE,
            session: DiagSession::DEFAULT,
            rx_total: 0,
            tx_total: 0,
            rx_dropped: 0,
            requests_handled: 0,
            responses_sent: 0,
            server_errors: 0,
            last_p2_us: 80,
            min_p2_us: 45,
            max_p2_us: 150,
            avg_p2_us: 80,
            latency_samples: 0,
            flashing: false,
            blocks_sent: 0,
            total_blocks: 1000,
            total_retries: 0,
            bytes_transferred: 0,
            flash_start_ms: 0,
            alert_seq: 0,
            bus_load_pct: 35,
        }
    }

    fn tick(&mut self, rng: &mut impl Rng) {
        self.rx_total += rng.gen_range(5..20);
        self.tx_total += rng.gen_range(3..15);
        self.requests_handled += rng.gen_range(2..10);
        self.responses_sent += rng.gen_range(2..10);

        if rng.gen_ratio(1, 200) {
            self.rx_dropped += 1;
        }
        if rng.gen_ratio(1, 500) {
            self.server_errors += 1;
        }

        let delta: i32 = rng.gen_range(-5..6);
        self.bus_load_pct = (self.bus_load_pct as i32 + delta).clamp(10, 95) as u32;

        let p2: u64 = rng.gen_range(40..200);
        self.last_p2_us = p2;
        if p2 < self.min_p2_us {
            self.min_p2_us = p2;
        }
        if p2 > self.max_p2_us {
            self.max_p2_us = p2;
        }
        self.latency_samples += 1;
        self.avg_p2_us =
            (self.avg_p2_us * (self.latency_samples - 1) + p2) / self.latency_samples;

        if self.flashing && self.blocks_sent < self.total_blocks {
            let blocks_this_tick = rng.gen_range(5..15);
            self.blocks_sent = (self.blocks_sent + blocks_this_tick).min(self.total_blocks);
            self.bytes_transferred += u64::from(blocks_this_tick) * 256;
            if rng.gen_ratio(1, 10) {
                self.total_retries += rng.gen_range(1..4);
            }
            if self.blocks_sent >= self.total_blocks {
                self.flashing = false;
                self.state = EcuState::IDLE;
                self.session = DiagSession::DEFAULT;
            }
        }

        if !self.flashing && self.state == EcuState::IDLE && rng.gen_ratio(1, 60) {
            self.flashing = true;
            self.state = EcuState::FLASHING;
            self.session = DiagSession::PROGRAMMING;
            self.blocks_sent = 0;
            self.total_blocks = rng.gen_range(500..2000);
            self.total_retries = 0;
            self.bytes_transferred = 0;
            self.flash_start_ms = now_ms();
        }
    }
}

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    println!("============================================================");
    println!("  HDDS UDS Fleet Diagnostic Bridge");
    println!("  HUDS (CAN/UDS) -> HDDS (DDS) distributed diagnostics");
    println!("============================================================");
    println!();

    let participant = Participant::builder("diag-bridge")
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    info!("DDS participant created on domain 0");

    let health_writer = participant.topic::<EcuHealth>("diag/ecu/health")?
        .writer()
        .qos(QoS::reliable().transient_local())
        .build()?;
    let latency_writer =
        participant.topic::<P2Latency>("diag/ecu/latency")?
            .writer()
            .qos(QoS::best_effort())
            .build()?;
    let transfer_writer =
        participant.topic::<TransferProgress>("diag/ecu/transfer")?
            .writer()
            .qos(QoS::reliable())
            .build()?;
    let alert_writer = participant.topic::<DiagAlert>("diag/fleet/anomalies")?
        .writer()
        .qos(QoS::reliable().keep_all())
        .build()?;
    let fleet_writer =
        participant.topic::<FleetStats>("diag/fleet/stats")?
            .writer()
            .qos(QoS::best_effort())
            .build()?;
    let session_writer =
        participant.topic::<SessionChange>("diag/ecu/session")?
            .writer()
            .qos(QoS::reliable())
            .build()?;

    info!("6 writers created (health, latency, transfer, alerts, fleet, session)");
    println!();

    let mut ecus = vec![
        SimulatedEcu::new("ecu-engine-01"),
        SimulatedEcu::new("ecu-brake-01"),
        SimulatedEcu::new("ecu-body-01"),
        SimulatedEcu::new("ecu-infotainment-01"),
    ];

    let mut rng = rand::thread_rng();
    let mut tick: u64 = 0;

    loop {
        tick += 1;
        let ts = now_ns();

        let mut ecus_flashing: u32 = 0;
        let mut ecus_error: u32 = 0;
        let mut total_requests: u64 = 0;
        let mut total_errors: u64 = 0;
        let mut total_bytes: u64 = 0;
        let mut bus_load_sum: u64 = 0;

        for ecu in &mut ecus {
            let prev_session = ecu.session;
            ecu.tick(&mut rng);

            if ecu.session != prev_session {
                let _ = session_writer.write(&SessionChange {
                    ecu_id: ecu.id.clone(),
                    previous_session: prev_session,
                    new_session: ecu.session,
                    timestamp_ns: ts,
                });
                info!(
                    "[{}] Session: {} -> {}",
                    ecu.id,
                    prev_session.name(),
                    ecu.session.name()
                );
            }

            let _ = health_writer.write(&EcuHealth {
                ecu_id: ecu.id.clone(),
                state: ecu.state,
                session: ecu.session,
                rx_total: ecu.rx_total,
                rx_dropped: ecu.rx_dropped,
                tx_total: ecu.tx_total,
                tx_deferred: 0,
                p2_violations: u32::from(ecu.last_p2_us > 150),
                bus_load_pct: ecu.bus_load_pct,
                requests_handled: ecu.requests_handled,
                responses_sent: ecu.responses_sent,
                server_errors: ecu.server_errors,
                timestamp_ns: ts,
            });

            let _ = latency_writer.write(&P2Latency {
                ecu_id: ecu.id.clone(),
                last_p2_us: ecu.last_p2_us,
                min_p2_us: ecu.min_p2_us,
                max_p2_us: ecu.max_p2_us,
                avg_p2_us: ecu.avg_p2_us,
                samples: ecu.latency_samples,
                timestamp_ns: ts,
            });

            if ecu.flashing {
                let _ = transfer_writer.write(&TransferProgress {
                    ecu_id: ecu.id.clone(),
                    blocks_sent: ecu.blocks_sent,
                    total_blocks: ecu.total_blocks,
                    progress_pct: ((ecu.blocks_sent as f64 / ecu.total_blocks as f64) * 100.0)
                        as u8,
                    total_retries: ecu.total_retries,
                    blocks_with_many_retries: ecu.total_retries / 3,
                    max_backoff_reached: 0,
                    elapsed_ms: now_ms() - ecu.flash_start_ms,
                    bytes_transferred: ecu.bytes_transferred,
                    timestamp_ns: ts,
                });
            }

            if rng.gen_ratio(1, 30) {
                ecu.alert_seq += 1;
                let severity = match rng.gen_range(0u32..10) {
                    0..=5 => AlertSeverity::WARNING,
                    6..=8 => AlertSeverity::ALERT_ERROR,
                    _ => AlertSeverity::CRITICAL,
                };
                let summary = match severity {
                    AlertSeverity::WARNING => "High retry rate on recent blocks",
                    AlertSeverity::ALERT_ERROR => "P2 timeout exceeded budget",
                    AlertSeverity::CRITICAL => "Flash sector write failure detected",
                    _ => "Normal operation",
                };
                let _ = alert_writer.write(&DiagAlert {
                    ecu_id: ecu.id.clone(),
                    alert_seq: ecu.alert_seq,
                    severity,
                    summary: summary.to_string(),
                    engine: "rules".to_string(),
                    analysis_time_ms: rng.gen_range(0..5),
                    timestamp_ns: ts,
                });
                info!("[{}] ALERT #{} [{}] {}", ecu.id, ecu.alert_seq, severity.name(), summary);
            }

            if ecu.flashing {
                ecus_flashing += 1;
            }
            if ecu.state == EcuState::ECU_ERROR {
                ecus_error += 1;
            }
            total_requests += ecu.requests_handled;
            total_errors += ecu.server_errors;
            total_bytes += ecu.bytes_transferred;
            bus_load_sum += u64::from(ecu.bus_load_pct);
        }

        let _ = fleet_writer.write(&FleetStats {
            bridge_id: "bridge-01".to_string(),
            ecus_online: ecus.len() as u32,
            ecus_flashing,
            ecus_error,
            total_requests,
            total_errors,
            total_bytes_transferred: total_bytes,
            avg_bus_load_pct: bus_load_sum as f64 / ecus.len() as f64,
            timestamp_ns: ts,
        });

        println!(
            "  [T+{:>4}s] ECUs={} | flash={} | bus={:.0}% | req={} | alerts={}",
            tick,
            ecus.len(),
            ecus_flashing,
            bus_load_sum as f64 / ecus.len() as f64,
            total_requests,
            ecus.iter().map(|e| e.alert_seq).sum::<u32>(),
        );

        std::thread::sleep(Duration::from_secs(1));
    }
}
