// CONCEPT DEMO - Simulated data for showcase purposes
//!
//! WAN Site Node Simulator
//!
//! Simulates a single WAN site that participates in NAT traversal.
//! Each site publishes heartbeats, ICE candidates, and messages to other sites.
//!
//! Usage:
//!   cargo run -p wan-site -- --site-name paris
//!   cargo run -p wan-site -- --site-name tokyo
//!   cargo run -p wan-site -- --site-name nyc

use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::time::{Duration, Instant};
use wan_types::{IceCandidate, SiteHeartbeat, SiteMessage};

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

/// Known sites with simulated geographic properties.
struct SiteProfile {
    name: &'static str,
    public_ip: &'static str,
    base_port: i32,
    nat_type: &'static str,
    /// Base RTT in ms to a central relay
    base_rtt: f64,
}

const SITES: &[SiteProfile] = &[
    SiteProfile {
        name: "paris",
        public_ip: "192.0.2.101",
        base_port: 7400,
        nat_type: "full_cone",
        base_rtt: 15.0,
    },
    SiteProfile {
        name: "tokyo",
        public_ip: "198.51.100.22",
        base_port: 7401,
        nat_type: "symmetric",
        base_rtt: 120.0,
    },
    SiteProfile {
        name: "nyc",
        public_ip: "203.0.113.33",
        base_port: 7402,
        nat_type: "restricted",
        base_rtt: 45.0,
    },
];

/// Compute simulated RTT between two sites based on their base RTT values.
fn compute_rtt(from: &SiteProfile, to: &SiteProfile) -> f64 {
    // Approximate: sum of base RTTs with some factor
    from.base_rtt + to.base_rtt + 5.0
}

/// Generate ICE candidates for a site.
fn generate_ice_candidates(profile: &SiteProfile) -> Vec<IceCandidate> {
    let mut candidates = Vec::new();

    // Host candidate (local LAN address)
    candidates.push(IceCandidate {
        candidate_id: format!("{}-host-0", profile.name),
        candidate_type: "host".to_string(),
        address: "192.168.1.100".to_string(),
        port: profile.base_port,
        priority: 2130706431, // Host priority
        site_name: profile.name.to_string(),
    });

    // Server-reflexive candidate (STUN-discovered public address)
    candidates.push(IceCandidate {
        candidate_id: format!("{}-srflx-0", profile.name),
        candidate_type: "srflx".to_string(),
        address: profile.public_ip.to_string(),
        port: profile.base_port + 1000,
        priority: 1694498815, // srflx priority
        site_name: profile.name.to_string(),
    });

    // Relay candidate (TURN relay)
    candidates.push(IceCandidate {
        candidate_id: format!("{}-relay-0", profile.name),
        candidate_type: "relay".to_string(),
        address: "203.0.113.1".to_string(), // Simulated TURN server
        port: 3478,
        priority: 16777215, // Relay priority (lowest)
        site_name: profile.name.to_string(),
    });

    candidates
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("wan_site=info".parse().unwrap()),
        )
        .init();

    // Parse CLI args
    let args: Vec<String> = std::env::args().collect();
    let site_name = args
        .iter()
        .position(|a| a == "--site-name")
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
        .unwrap_or("paris");

    let profile = SITES
        .iter()
        .find(|s| s.name == site_name)
        .unwrap_or_else(|| {
            tracing::warn!(
                "Unknown site '{}', defaulting to paris. Known: paris, tokyo, nyc",
                site_name
            );
            &SITES[0]
        });

    tracing::info!(
        "Starting WAN site: {} (IP: {}, NAT: {})",
        profile.name,
        profile.public_ip,
        profile.nat_type
    );

    // Create DDS participant
    let participant = Participant::builder(&format!("wan-site-{}", profile.name))
        .with_transport(TransportMode::UdpMulticast)
        .domain_id(DOMAIN_ID)
        .build()?;

    // Create writers
    let heartbeat_writer =
        participant.topic::<SiteHeartbeat>("wan/heartbeat")?
            .writer()
            .qos(QoS::default())
            .build()?;
    let ice_writer =
        participant.topic::<IceCandidate>("wan/ice_candidates")?
            .writer()
            .qos(QoS::default())
            .build()?;
    let msg_writer =
        participant.topic::<SiteMessage>("wan/messages")?
            .writer()
            .qos(QoS::default())
            .build()?;

    // Publish ICE candidates once at startup
    let candidates = generate_ice_candidates(profile);
    for candidate in &candidates {
        ice_writer.write(candidate)?;
        tracing::info!(
            "Published ICE candidate: {} type={} addr={}:{}",
            candidate.candidate_id,
            candidate.candidate_type,
            candidate.address,
            candidate.port
        );
    }

    // Other sites to send messages to
    let other_sites: Vec<&SiteProfile> = SITES.iter().filter(|s| s.name != profile.name).collect();

    let mut rng = rand::thread_rng();
    let mut heartbeat_seq: i64 = 0;
    let mut msg_seq: i64 = 0;
    let start = Instant::now();

    // Determine if this site uses a relay (symmetric NAT often requires relay)
    let using_relay = profile.nat_type == "symmetric";
    const STATIC_REFRESH_S: f32 = 2.0;
    let mut last_candidate_publish = std::time::Instant::now();

    loop {
        let now_ns = start.elapsed().as_nanos() as i64;

        // Static data, re-sent for readers that join later: a reader
        // only receives what is written after discovery matched it.
        let tick_now = std::time::Instant::now();
        if tick_now.duration_since(last_candidate_publish).as_secs_f32() > STATIC_REFRESH_S {
            for candidate in &candidates {
                ice_writer.write(candidate)?;
            }
            last_candidate_publish = tick_now;
        }

        // Publish heartbeat at 1 Hz
        let jitter: f64 = rng.gen_range(-2.0..2.0);
        let heartbeat = SiteHeartbeat {
            site_name: profile.name.to_string(),
            public_ip: profile.public_ip.to_string(),
            public_port: profile.base_port + rng.gen_range(0..100),
            nat_type: profile.nat_type.to_string(),
            using_relay,
            rtt_ms: profile.base_rtt + jitter,
            timestamp_ns: now_ns,
        };
        heartbeat_writer.write(&heartbeat)?;
        heartbeat_seq += 1;

        if heartbeat_seq % 5 == 0 {
            tracing::info!(
                "[{}] Heartbeat #{}: RTT={:.1}ms relay={}",
                profile.name,
                heartbeat_seq,
                heartbeat.rtt_ms,
                using_relay
            );
        }

        // Publish messages to other sites at ~0.5 Hz (every 2 heartbeats)
        if heartbeat_seq % 2 == 0 {
            for other in &other_sites {
                let rtt = compute_rtt(profile, other);
                let rtt_jitter: f64 = rng.gen_range(-5.0..5.0);
                let simulated_rtt_ns = ((rtt + rtt_jitter) * 1_000_000.0) as i64;

                let msg = SiteMessage {
                    seq: msg_seq,
                    from_site: profile.name.to_string(),
                    to_site: other.name.to_string(),
                    payload_size: rng.gen_range(64..1400),
                    sent_ns: now_ns,
                    received_ns: now_ns + simulated_rtt_ns,
                };
                msg_writer.write(&msg)?;
                msg_seq += 1;

                tracing::debug!(
                    "[{}] -> [{}] msg #{} RTT={:.1}ms",
                    profile.name,
                    other.name,
                    msg.seq,
                    rtt + rtt_jitter
                );
            }
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
