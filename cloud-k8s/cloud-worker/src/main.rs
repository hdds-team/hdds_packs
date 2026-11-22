// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! Cloud Worker - Simulated K8s Pod
//!
//! Publishes WorkloadMetric at 2Hz with realistic CPU/memory fluctuations.
//! Publishes DiscoveryEvent on startup (joined) and shutdown (left).
//! Simulates load spikes every ~30s.

use clap::Parser;
use cloud_types::{timestamp_ns, topics, DiscoveryEvent, WorkloadMetric};
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::time::{Duration, Instant};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

#[derive(Parser)]
#[command(name = "cloud-worker", about = "Simulated K8s pod worker for HDDS demo")]
struct Args {
    #[arg(long, default_value = "worker-1")]
    pod_name: String,
    #[arg(long, default_value = "default")]
    namespace: String,
    #[arg(long, default_value = "node-a")]
    node_name: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    tracing::info!("==========================================");
    tracing::info!("  HDDS Cloud Worker - K8s Pod Simulator");
    tracing::info!("  Pod: {}  Node: {}", args.pod_name, args.node_name);
    tracing::info!("==========================================");

    // Create DDS participant
    let participant = Participant::builder(&format!("cloud_worker_{}", args.pod_name))
        .domain_id(DOMAIN_ID)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::default();

    // The join and leave events are published once each, so a monitor that
    // connects in between would never learn this pod exists: a reader only
    // receives what is written after discovery matched it. Keeping them for
    // late joiners is what TRANSIENT_LOCAL durability is for.
    let mut discovery_qos = QoS::default();
    discovery_qos.durability = hdds::qos::Durability::TransientLocal;

    // Create writers
    let metric_writer =
        participant.topic::<WorkloadMetric>(topics::WORKLOAD_METRIC)?
            .writer()
            .qos(qos)
            .build()?;
    let discovery_writer =
        participant.topic::<DiscoveryEvent>(topics::DISCOVERY_EVENT)?
            .writer()
            .qos(discovery_qos)
            .build()?;

    // Generate a fake participant GUID
    let participant_guid = format!(
        "{:08x}-{:04x}-{:04x}",
        rand::thread_rng().gen::<u32>(),
        rand::thread_rng().gen::<u16>(),
        rand::thread_rng().gen::<u16>()
    );

    // Publish join event
    let join_event = DiscoveryEvent {
        event_id: format!("evt-join-{}", args.pod_name),
        participant_guid: participant_guid.clone(),
        discovery_method: "k8s".to_string(),
        locator: format!("udp/239.255.0.1:7400#{}", args.pod_name),
        joined: true,
        timestamp_ns: timestamp_ns(),
    };
    discovery_writer.write(&join_event)?;
    tracing::info!("Published join event for {}", args.pod_name);

    // Simulation state
    let mut rng = rand::thread_rng();
    let start_time = Instant::now();
    let mut base_cpu: f64 = rng.gen_range(15.0..35.0);
    let mut base_memory: f64 = rng.gen_range(100.0..200.0);
    let mut tick: u64 = 0;
    let spike_interval_s: f64 = 30.0;
    let mut next_spike_at = spike_interval_s + rng.gen_range(-5.0..5.0);

    // Install ctrl-c handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc_handler(r);

    tracing::info!("Publishing metrics at 2Hz. Press Ctrl+C to stop.");

    while running.load(std::sync::atomic::Ordering::Relaxed) {
        let elapsed = start_time.elapsed().as_secs_f64();
        tick += 1;

        // CPU: sinusoidal base + noise + spikes
        let sin_component = ((elapsed * 0.1).sin() * 15.0).abs();
        let noise = rng.gen_range(-3.0..3.0);
        let spike = if (elapsed - next_spike_at).abs() < 5.0 {
            // During a spike, CPU jumps significantly
            30.0 + rng.gen_range(0.0..20.0)
        } else {
            0.0
        };
        let cpu_percent = (base_cpu + sin_component + noise + spike).clamp(5.0, 98.0);

        // Schedule next spike
        if elapsed > next_spike_at + 5.0 {
            next_spike_at = elapsed + spike_interval_s + rng.gen_range(-5.0..5.0);
        }

        // Memory: slowly growing with small fluctuations
        base_memory += rng.gen_range(-0.5..1.0);
        base_memory = base_memory.clamp(100.0, 500.0);
        let memory_mb = base_memory + rng.gen_range(-5.0..5.0);

        // Connections and throughput scale with CPU
        let active_connections = ((cpu_percent / 10.0) as i32 + rng.gen_range(0..5)).max(1);
        let messages_per_second =
            ((cpu_percent * 10.0) as i32 + rng.gen_range(-50..50)).max(10);

        let metric = WorkloadMetric {
            pod_name: args.pod_name.clone(),
            namespace: args.namespace.clone(),
            node_name: args.node_name.clone(),
            cpu_percent,
            memory_mb,
            active_connections,
            messages_per_second,
            timestamp_ns: timestamp_ns(),
        };

        metric_writer.write(&metric)?;

        if tick % 10 == 0 {
            tracing::info!(
                "[{}] CPU: {:.1}%  Mem: {:.0}MB  Conn: {}  Msg/s: {}",
                args.pod_name,
                cpu_percent,
                memory_mb,
                active_connections,
                messages_per_second
            );
        }

        // Slowly drift base CPU
        base_cpu += rng.gen_range(-0.5..0.5);
        base_cpu = base_cpu.clamp(10.0, 60.0);

        std::thread::sleep(Duration::from_millis(500)); // 2Hz
    }

    // Publish leave event on shutdown
    tracing::info!("Shutting down, publishing leave event...");
    let leave_event = DiscoveryEvent {
        event_id: format!("evt-leave-{}", args.pod_name),
        participant_guid,
        discovery_method: "k8s".to_string(),
        locator: format!("udp/239.255.0.1:7400#{}", args.pod_name),
        joined: false,
        timestamp_ns: timestamp_ns(),
    };
    discovery_writer.write(&leave_event)?;

    tracing::info!("Worker {} stopped.", args.pod_name);
    Ok(())
}

/// Set up a ctrl-c handler that sets the running flag to false
fn ctrlc_handler(running: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    // Best-effort signal handling - ignore errors on platforms without signal support
    let _ = std::thread::spawn(move || {
        // Simple polling approach for portability
        // In a real app, use the `ctrlc` crate or tokio signals
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if !running.load(std::sync::atomic::Ordering::Relaxed) {
                break;
            }
        }
    });
}
