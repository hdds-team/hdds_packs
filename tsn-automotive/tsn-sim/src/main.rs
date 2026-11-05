// CONCEPT DEMO - Simulated data for showcase purposes
//
// TSN Automotive ECU Simulator
//
// Simulates an autonomous vehicle's ECU network publishing data at different
// TSN priority tiers over HDDS/DDS with real QoS-to-TSN mapping:
//
//   CRITICAL (100Hz): Brake commands, Steering commands  -> PCP 7, DSCP 46 (EF), deadline 1ms
//   HIGH     (20Hz):  Vehicle telemetry                  -> PCP 4, DSCP 34 (AF41), deadline 10ms
//   LOW      (1Hz):   TSN network metrics per tier       -> PCP 0, best-effort

use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use hdds::{Participant, QoS, TransportMode};
use hdds::dds::Deadline;
use hdds::transport::tsn::{TsnConfig, TsnProbe, TxTimePolicy, qos_to_tsn, QosHints};
use rand::Rng;
use tsn_types::*;

/// Get current timestamp in nanoseconds since Unix epoch.
fn now_ns() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

/// Map our IDL PriorityTier to a real TSN QoS profile.
fn qos_for_tier(tier: PriorityTier) -> QoS {
    match tier {
        PriorityTier::CRITICAL => {
            // Safety-critical: reliable, high transport priority, tight deadline
            QoS::reliable()
                .transport_priority(100)  // Maps to PCP 7 via qos_to_tsn
                .deadline(Deadline { period: Duration::from_millis(1) })
        }
        PriorityTier::HIGH => {
            // Sensor telemetry: reliable, normal priority, moderate deadline
            QoS::reliable()
                .transport_priority(50)   // Maps to PCP 4 via qos_to_tsn
                .deadline(Deadline { period: Duration::from_millis(10) })
        }
        PriorityTier::MEDIUM => {
            // Diagnostics: reliable, reduced priority
            QoS::reliable()
                .transport_priority(25)   // Maps to PCP 2 via qos_to_tsn
                .deadline(Deadline { period: Duration::from_millis(50) })
        }
        PriorityTier::LOW => {
            // Metrics/monitoring: best-effort, no deadline constraint
            QoS::default()
                .transport_priority_low()
        }
    }
}

/// Build QosHints from a PriorityTier for TSN auto-mapping display.
fn hints_for_tier(tier: PriorityTier) -> QosHints {
    match tier {
        PriorityTier::CRITICAL => QosHints::reliable_no_deadline()
            .with_priority(100)
            .with_deadline(Duration::from_millis(1)),
        PriorityTier::HIGH => QosHints::reliable_no_deadline()
            .with_priority(50)
            .with_deadline(Duration::from_millis(10)),
        PriorityTier::MEDIUM => QosHints::reliable_no_deadline()
            .with_priority(25)
            .with_deadline(Duration::from_millis(50)),
        PriorityTier::LOW => QosHints::best_effort(),
    }
}

/// Pretty-print a TsnConfig.
fn print_tsn_config(label: &str, tier: PriorityTier, cfg: &TsnConfig) {
    let pcp_str = cfg.pcp.map_or("none".to_string(), |p| p.to_string());
    let dscp_str = cfg.dscp.map_or("none".to_string(), |d| {
        let name = match d {
            46 => "EF",
            34 => "AF41",
            0 => "BE",
            _ => "??",
        };
        format!("{} ({})", d, name)
    });
    let tc_str = cfg.traffic_class.map_or("n/a".to_string(), |t| t.to_string());
    let txtime_str = match cfg.tx_time {
        TxTimePolicy::Disabled => "off",
        TxTimePolicy::Opportunistic => "opportunistic",
        TxTimePolicy::Mandatory => "mandatory",
    };
    println!(
        "  {:16} {:8}  PCP={:<4} DSCP={:<10} TC={:<4} TxTime={}",
        label,
        format!("{:?}", tier),
        pcp_str,
        dscp_str,
        tc_str,
        txtime_str,
    );
}

/// Simulated vehicle state that evolves over time.
struct VehicleState {
    speed_kmh: f64,
    rpm: f64,
    fuel_percent: f64,
    battery_volt: f64,
    oil_temp_c: f64,
    steering_angle: f64,
    brake_pressure: f64,
    abs_active: bool,
    emergency: bool,
    phase: f64,
}

impl VehicleState {
    fn new() -> Self {
        Self {
            speed_kmh: 60.0,
            rpm: 2500.0,
            fuel_percent: 85.0,
            battery_volt: 13.8,
            oil_temp_c: 90.0,
            steering_angle: 0.0,
            brake_pressure: 0.0,
            abs_active: false,
            emergency: false,
            phase: 0.0,
        }
    }

    fn step(&mut self, dt: f64, rng: &mut impl Rng) {
        self.phase += dt;
        let t = self.phase;

        let base_speed = 85.0 + 45.0 * (t * 0.15).sin();
        self.speed_kmh = (base_speed + rng.gen_range(-2.0..2.0)).clamp(0.0, 130.0);

        let gear_ratio = if self.speed_kmh < 30.0 {
            3.5
        } else if self.speed_kmh < 60.0 {
            2.2
        } else if self.speed_kmh < 90.0 {
            1.5
        } else {
            1.0
        };
        self.rpm = (self.speed_kmh * gear_ratio * 40.0 + rng.gen_range(-50.0..50.0))
            .clamp(800.0, 7000.0);

        self.fuel_percent = (self.fuel_percent - dt * 0.01).max(5.0);

        self.battery_volt = 13.8 + 0.3 * (t * 2.0).sin() + rng.gen_range(-0.05..0.05);
        self.battery_volt = self.battery_volt.clamp(12.0, 14.8);

        let target_oil = 85.0 + self.rpm / 700.0 * 5.0;
        self.oil_temp_c += (target_oil - self.oil_temp_c) * dt * 0.1;
        self.oil_temp_c += rng.gen_range(-0.2..0.2);
        self.oil_temp_c = self.oil_temp_c.clamp(60.0, 130.0);

        self.steering_angle = 12.0 * (t * 0.3 * std::f64::consts::PI).sin()
            + 3.0 * (t * 0.7).sin()
            + rng.gen_range(-0.5..0.5);
        self.steering_angle = self.steering_angle.clamp(-45.0, 45.0);

        let brake_cycle = (t * 0.1).sin();
        if brake_cycle > 0.7 {
            self.brake_pressure = 30.0 + 50.0 * (brake_cycle - 0.7) / 0.3;
            self.brake_pressure += rng.gen_range(-2.0..2.0);
            self.brake_pressure = self.brake_pressure.clamp(0.0, 120.0);
        } else {
            self.brake_pressure = (self.brake_pressure - dt * 80.0).max(0.0);
        }

        self.abs_active = self.brake_pressure > 60.0 && rng.gen_bool(0.3);

        self.emergency = rng.gen_bool(0.001);
        if self.emergency {
            self.brake_pressure = 120.0;
            self.abs_active = true;
        }
    }
}

/// Simulated TSN metrics tracker per priority tier.
struct TierTracker {
    tier: PriorityTier,
    interface_name: String,
    packets_sent: i64,
    packets_on_time: i64,
    packets_late: i64,
    latencies_us: Vec<f64>,
}

impl TierTracker {
    fn new(tier: PriorityTier, interface_name: &str) -> Self {
        Self {
            tier,
            interface_name: interface_name.to_string(),
            packets_sent: 0,
            packets_on_time: 0,
            packets_late: 0,
            latencies_us: Vec::with_capacity(1024),
        }
    }

    fn record_packet(&mut self, rng: &mut impl Rng) {
        self.packets_sent += 1;

        let (base_us, jitter_us, deadline_us) = match self.tier {
            PriorityTier::CRITICAL => (200.0, 100.0, 1000.0),
            PriorityTier::HIGH => (800.0, 400.0, 5000.0),
            PriorityTier::MEDIUM => (2000.0, 1500.0, 50000.0),
            PriorityTier::LOW => (5000.0, 3000.0, 100000.0),
        };

        let latency = base_us + rng.gen_range(0.0..jitter_us);
        let latency = if rng.gen_bool(0.01) {
            latency * 3.0
        } else {
            latency
        };

        self.latencies_us.push(latency);

        if latency <= deadline_us {
            self.packets_on_time += 1;
        } else {
            self.packets_late += 1;
        }
    }

    fn p99_latency_us(&self) -> f64 {
        if self.latencies_us.is_empty() {
            return 0.0;
        }
        let mut sorted = self.latencies_us.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let idx = ((sorted.len() as f64) * 0.99) as usize;
        let idx = idx.min(sorted.len() - 1);
        sorted[idx]
    }

    fn jitter_us(&self) -> f64 {
        if self.latencies_us.len() < 2 {
            return 0.0;
        }
        let n = self.latencies_us.len() as f64;
        let mean = self.latencies_us.iter().sum::<f64>() / n;
        let variance = self.latencies_us.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        variance.sqrt()
    }

    fn to_metrics(&mut self) -> TsnMetrics {
        let m = TsnMetrics {
            interface_name: self.interface_name.clone(),
            tier: self.tier,
            packets_sent: self.packets_sent,
            packets_on_time: self.packets_on_time,
            packets_late: self.packets_late,
            p99_latency_us: self.p99_latency_us(),
            jitter_us: self.jitter_us(),
            timestamp_ns: now_ns(),
        };
        self.latencies_us.clear();
        m
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    println!("============================================================");
    println!("  HDDS TSN Automotive ECU Simulator");
    println!("  Priority tiers: CRITICAL(100Hz) HIGH(20Hz) LOW(1Hz)");
    println!("============================================================");
    println!();

    // --- Phase 1: Probe TSN capabilities on the system ---
    let tsn_caps = TsnProbe::probe("eth0").unwrap_or_default();
    println!("TSN Capabilities (eth0):");
    println!("  SO_TXTIME      : {:?}", tsn_caps.so_txtime);
    println!("  HW Timestamping: {:?}", tsn_caps.hw_timestamping);
    println!("  ETF qdisc      : {}", tsn_caps.etf_configured);
    println!("  TAPRIO qdisc   : {}", tsn_caps.taprio_configured);
    println!("  MQPRIO qdisc   : {}", tsn_caps.mqprio_configured);
    if tsn_caps.is_tsn_ready() {
        println!("  Status         : TSN READY");
    } else {
        println!("  Status         : No HW TSN (BestEffort fallback)");
    }
    println!();

    // --- Phase 2: Show QoS-to-TSN mapping per tier ---
    println!("QoS -> TSN auto-mapping (via qos_to_tsn):");
    let tiers = [
        ("tsn/brake", PriorityTier::CRITICAL),
        ("tsn/steering", PriorityTier::CRITICAL),
        ("tsn/telemetry", PriorityTier::HIGH),
        ("tsn/metrics", PriorityTier::LOW),
    ];
    for (topic, tier) in &tiers {
        let hints = hints_for_tier(*tier);
        let tsn_cfg = qos_to_tsn(&hints, &tsn_caps);
        print_tsn_config(topic, *tier, &tsn_cfg);
    }
    println!();

    // --- Phase 3: Create DDS participant ---
    let participant = Participant::builder("tsn-ecu-sim")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    println!("Participant: {}", participant.name());
    println!("Domain ID:   {}", participant.domain_id());
    println!();

    // --- Phase 4: Create writers with real TSN QoS per tier ---
    let brake_writer = participant.topic::<BrakeCommand>("tsn/brake")?
        .writer()
        .qos(qos_for_tier(PriorityTier::CRITICAL))
        .build()?;

    let steering_writer = participant.topic::<SteeringCommand>("tsn/steering")?
        .writer()
        .qos(qos_for_tier(PriorityTier::CRITICAL))
        .build()?;

    let telemetry_writer = participant.topic::<VehicleTelemetry>("tsn/telemetry")?
        .writer()
        .qos(qos_for_tier(PriorityTier::HIGH))
        .build()?;

    let metrics_writer = participant.topic::<TsnMetrics>("tsn/metrics")?
        .writer()
        .qos(qos_for_tier(PriorityTier::LOW))
        .build()?;

    println!("Writers created with TSN-aware QoS:");
    println!("  tsn/brake     CRITICAL  reliable  deadline=1ms   transport_priority=100");
    println!("  tsn/steering  CRITICAL  reliable  deadline=1ms   transport_priority=100");
    println!("  tsn/telemetry HIGH      reliable  deadline=10ms  transport_priority=50");
    println!("  tsn/metrics   LOW       best-eff  no deadline    transport_priority=-50");
    println!();
    println!("Simulation running... (Ctrl+C to stop)");
    println!();

    let mut rng = rand::thread_rng();
    let mut vehicle = VehicleState::new();

    // TSN metrics trackers
    let mut critical_tracker = TierTracker::new(PriorityTier::CRITICAL, "eth0.tsn0");
    let mut high_tracker = TierTracker::new(PriorityTier::HIGH, "eth0.tsn1");
    let mut medium_tracker = TierTracker::new(PriorityTier::MEDIUM, "eth0.tsn2");
    let mut low_tracker = TierTracker::new(PriorityTier::LOW, "eth0.tsn3");

    let sim_start = Instant::now();
    let mut last_critical = Instant::now();
    let mut last_high = Instant::now();
    let mut last_low = Instant::now();
    let mut cycle_count: u64 = 0;

    // Main simulation loop at ~1000Hz internal tick
    loop {
        let loop_start = Instant::now();
        let dt = 0.001; // 1ms tick

        vehicle.step(dt, &mut rng);

        // CRITICAL tier: 100Hz (every 10ms)
        if last_critical.elapsed() >= Duration::from_millis(10) {
            last_critical = Instant::now();

            let brake = BrakeCommand {
                ecu_id: 1,
                pressure_bar: vehicle.brake_pressure,
                abs_active: vehicle.abs_active,
                emergency: vehicle.emergency,
                timestamp_ns: now_ns(),
            };
            brake_writer.write(&brake)?;
            critical_tracker.record_packet(&mut rng);

            let steering = SteeringCommand {
                ecu_id: 2,
                angle_deg: vehicle.steering_angle,
                torque_nm: vehicle.steering_angle.abs() * 0.8 + rng.gen_range(0.0..2.0),
                timestamp_ns: now_ns(),
            };
            steering_writer.write(&steering)?;
            critical_tracker.record_packet(&mut rng);

            if vehicle.emergency {
                println!(
                    "  [EMERGENCY] Brake={:.1}bar ABS={} speed={:.1}km/h",
                    vehicle.brake_pressure, vehicle.abs_active, vehicle.speed_kmh
                );
            }
        }

        // HIGH tier: 20Hz (every 50ms)
        if last_high.elapsed() >= Duration::from_millis(50) {
            last_high = Instant::now();

            let telemetry = VehicleTelemetry {
                vehicle_id: 1,
                speed_kmh: vehicle.speed_kmh,
                rpm: vehicle.rpm,
                fuel_percent: vehicle.fuel_percent,
                battery_volt: vehicle.battery_volt,
                oil_temp_c: vehicle.oil_temp_c,
                timestamp_ns: now_ns(),
            };
            telemetry_writer.write(&telemetry)?;
            high_tracker.record_packet(&mut rng);
        }

        // LOW tier: 1Hz (every 1000ms) - TSN metrics
        if last_low.elapsed() >= Duration::from_secs(1) {
            last_low = Instant::now();

            for _ in 0..5 {
                medium_tracker.record_packet(&mut rng);
            }
            low_tracker.record_packet(&mut rng);

            let metrics_list = [
                critical_tracker.to_metrics(),
                high_tracker.to_metrics(),
                medium_tracker.to_metrics(),
                low_tracker.to_metrics(),
            ];

            for m in &metrics_list {
                metrics_writer.write(m)?;
            }

            let elapsed = sim_start.elapsed().as_secs();
            println!(
                "  [T+{:>4}s] speed={:>5.1}km/h rpm={:>6.0} fuel={:>5.1}% brake={:>5.1}bar steer={:>+6.1}deg",
                elapsed,
                vehicle.speed_kmh,
                vehicle.rpm,
                vehicle.fuel_percent,
                vehicle.brake_pressure,
                vehicle.steering_angle,
            );
        }

        cycle_count += 1;

        let elapsed = loop_start.elapsed();
        if elapsed < Duration::from_millis(1) {
            thread::sleep(Duration::from_millis(1) - elapsed);
        }

        if sim_start.elapsed() > Duration::from_secs(300) {
            println!();
            println!("Simulation complete ({} cycles)", cycle_count);
            break;
        }
    }

    Ok(())
}
