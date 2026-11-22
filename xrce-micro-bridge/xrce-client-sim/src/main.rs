// CONCEPT DEMO - Simulated data for showcase purposes
//
// XRCE Micro-Client Simulator
//
// Simulates 4 resource-constrained XRCE micro-clients (ESP32-like sensors)
// that would normally communicate via the DDS-XRCE protocol over UDP/Serial.
//
// In this demo, each simulated client publishes MicroSensorData directly
// on the DDS topic "xrce/sensors" at ~1Hz, mimicking what the XRCE agent
// would produce after bridging real XRCE micro-client traffic.
//
// Simulated clients:
//   - client-temp-01:  Temperature sensor (20-35 C, noisy Brownian drift)
//   - client-humid-01: Humidity sensor (30-80 %RH, slow drift)
//   - client-press-01: Pressure sensor (980-1030 hPa, very slow drift)
//   - client-light-01: Light sensor (0-10000 lux, day/night sinusoidal cycle)

use std::time::Duration;

use hdds::{Participant, QoS, TransportMode};
use xrce_types::MicroSensorData;

use clap::Parser;
use rand::Rng;

/// DDS domain for the demo packs, from the project-wide domain table.
const DOMAIN_ID: u32 = 99;

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "xrce-client-sim",
    about = "Simulates 4 XRCE micro-clients publishing sensor data"
)]
struct Cli {
    /// DDS domain ID (must match agent)
    #[arg(long, default_value_t = DOMAIN_ID)]
    domain_id: u32,

    /// Publish rate in Hz per client
    #[arg(long, default_value_t = 1)]
    rate_hz: u32,

    /// Run for N seconds then exit (0 = run forever)
    #[arg(long, default_value_t = 0)]
    duration_secs: u64,
}

// ---------------------------------------------------------------------------
// Sensor simulators
// ---------------------------------------------------------------------------

/// Description of a simulated sensor client.
struct SensorSim {
    client_id: &'static str,
    sensor_type: &'static str,
    unit: &'static str,
    min_val: f64,
    max_val: f64,
    /// Current value (drifts over time)
    current: f64,
    /// Battery level (drains slowly)
    battery: f32,
    /// RSSI (fluctuates slightly)
    rssi: i32,
    /// Whether this sensor uses a sinusoidal cycle instead of Brownian drift
    sinusoidal: bool,
    /// Phase accumulator for sinusoidal sensors
    phase: f64,
}

impl SensorSim {
    fn new(
        client_id: &'static str,
        sensor_type: &'static str,
        unit: &'static str,
        min_val: f64,
        max_val: f64,
        initial: f64,
        sinusoidal: bool,
    ) -> Self {
        Self {
            client_id,
            sensor_type,
            unit,
            min_val,
            max_val,
            current: initial,
            battery: 100.0,
            rssi: -65,
            sinusoidal,
            phase: 0.0,
        }
    }

    /// Advance the simulation by one tick and return the new reading.
    fn tick(&mut self, rng: &mut impl Rng) -> MicroSensorData {
        if self.sinusoidal {
            // Day/night cycle: full cycle over ~120 ticks (2 minutes at 1Hz)
            self.phase += std::f64::consts::PI * 2.0 / 120.0;
            let mid = (self.min_val + self.max_val) / 2.0;
            let amplitude = (self.max_val - self.min_val) / 2.0;
            self.current = mid + amplitude * self.phase.sin();
            // Add a little noise
            self.current += rng.gen_range(-amplitude * 0.02..amplitude * 0.02);
        } else {
            // Brownian drift with mean reversion
            let mid = (self.min_val + self.max_val) / 2.0;
            let range = self.max_val - self.min_val;
            let noise = rng.gen_range(-range * 0.03..range * 0.03);
            let reversion = (mid - self.current) * 0.02;
            self.current += noise + reversion;
            self.current = self.current.clamp(self.min_val, self.max_val);
        }

        // Battery drain: ~0.01% per tick (slow)
        self.battery -= rng.gen_range(0.005..0.015);
        if self.battery < 0.0 {
            self.battery = 0.0;
        }

        // RSSI fluctuation
        self.rssi = (-65 + rng.gen_range(-5..5_i32)).clamp(-90, -30);

        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as i64;

        MicroSensorData {
            client_id: self.client_id.to_string(),
            sensor_type: self.sensor_type.to_string(),
            value: self.current,
            unit: self.unit.to_string(),
            battery_percent: self.battery,
            rssi_dbm: self.rssi,
            timestamp_ns,
        }
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter("xrce_client_sim=info,hdds=warn")
        .init();

    tracing::info!("Starting XRCE client simulator (rate={}Hz)", cli.rate_hz);

    // Create DDS participant (build() returns Arc<Participant>)
    let participant = Participant::builder("xrce-client-sim")
        .domain_id(cli.domain_id)
        .with_transport(TransportMode::UdpMulticast)
        .build()
        .expect("Failed to create DDS participant");

    tracing::info!("DDS participant created on domain {}", cli.domain_id);

    // Create writer for sensor data
    let sensor_writer = participant
        .topic::<MicroSensorData>("xrce/sensors")
        .and_then(|t| t.writer().qos(QoS::default()).build())
        .expect("Failed to create sensor writer");

    tracing::info!("Writer created on topic 'xrce/sensors'");

    // Initialize the 4 simulated sensors
    let mut sensors = vec![
        SensorSim::new(
            "client-temp-01",
            "temperature",
            "C",
            20.0,
            35.0,
            25.0,
            false,
        ),
        SensorSim::new(
            "client-humid-01",
            "humidity",
            "%RH",
            30.0,
            80.0,
            55.0,
            false,
        ),
        SensorSim::new(
            "client-press-01",
            "pressure",
            "hPa",
            980.0,
            1030.0,
            1013.25,
            false,
        ),
        SensorSim::new(
            "client-light-01",
            "light",
            "lux",
            0.0,
            10000.0,
            5000.0,
            true,
        ),
    ];

    let tick_interval = Duration::from_millis(1000 / cli.rate_hz.max(1) as u64);
    let start = std::time::Instant::now();
    let mut rng = rand::thread_rng();
    let mut tick_count: u64 = 0;

    tracing::info!(
        "Publishing sensor data every {:?} (4 clients x {}Hz)",
        tick_interval,
        cli.rate_hz
    );

    loop {
        // Check duration limit
        if cli.duration_secs > 0 && start.elapsed().as_secs() >= cli.duration_secs {
            tracing::info!(
                "Duration limit reached ({} seconds). Exiting.",
                cli.duration_secs
            );
            break;
        }

        // Publish one sample per sensor
        for sensor in &mut sensors {
            let sample = sensor.tick(&mut rng);

            match sensor_writer.write(&sample) {
                Ok(()) => {
                    if tick_count % 10 == 0 {
                        tracing::info!(
                            "[{}] {} = {:.2} {} (bat={:.1}%, rssi={}dBm)",
                            sample.client_id,
                            sample.sensor_type,
                            sample.value,
                            sample.unit,
                            sample.battery_percent,
                            sample.rssi_dbm,
                        );
                    }
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to publish for {}: {}",
                        sensor.client_id,
                        e
                    );
                }
            }
        }

        tick_count += 1;
        std::thread::sleep(tick_interval);
    }

    tracing::info!(
        "Client simulator finished. Published {} ticks ({} total samples).",
        tick_count,
        tick_count * 4,
    );

    Ok(())
}
