//! Predictive Maintenance Sensor Simulator
//!
//! Generates simulated industrial sensor data:
//! - Multiple machines with various sensors
//! - Normal operation with realistic noise
//! - Anomaly injection (degradation, spikes, failures)
//! - Alerts and maintenance predictions

use anyhow::Result;
use hdds::{Participant, QoS, TransportMode};
use log::*;
use rand::Rng;
use sensor_types::maintenance::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

const SIM_TICK_MS: u64 = 200; // 5 Hz update rate

fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

fn sensor_type_name(st: SensorType) -> &'static str {
    match st {
        SensorType::TEMPERATURE => "TEMPERATURE",
        SensorType::VIBRATION => "VIBRATION",
        SensorType::PRESSURE => "PRESSURE",
        SensorType::CURRENT => "CURRENT",
        SensorType::RPM => "RPM",
        SensorType::FLOW_RATE => "FLOW_RATE",
    }
}

fn sensor_type_unit(st: SensorType) -> &'static str {
    match st {
        SensorType::TEMPERATURE => "C",
        SensorType::VIBRATION => "mm/s",
        SensorType::PRESSURE => "bar",
        SensorType::CURRENT => "A",
        SensorType::RPM => "rpm",
        SensorType::FLOW_RATE => "L/min",
    }
}

mod topics {
    pub const SENSOR_READING: &str = "rt/maintenance/SensorReading";
    pub const MACHINE_STATUS: &str = "rt/maintenance/MachineStatus";
    pub const ALERT: &str = "rt/maintenance/Alert";
    pub const PREDICTION: &str = "rt/maintenance/MaintenancePrediction";
}

/// Sensor configuration
struct SensorConfig {
    sensor_id: u32,
    sensor_type: SensorType,
    nominal: f32,
    noise: f32,
    min_threshold: f32,
    max_threshold: f32,
}

/// Machine with sensors and health state
struct Machine {
    machine_id: u32,
    name: String,
    sensors: Vec<SensorConfig>,
    health_score: f32,
    operating_hours: f32,
    status: MachineState,
    // Degradation state
    degradation_factor: f32,
    degradation_rate: f32,
    degrading_sensor: Option<u32>,
    // Anomaly state
    anomaly_active: bool,
    anomaly_sensor: Option<u32>,
    anomaly_magnitude: f32,
    anomaly_end: Option<Instant>,
}

impl Machine {
    fn new(machine_id: u32, name: &str, sensors: Vec<SensorConfig>) -> Self {
        Machine {
            machine_id,
            name: name.to_string(),
            sensors,
            health_score: 100.0,
            operating_hours: rand::thread_rng().gen_range(100.0..5000.0),
            status: MachineState::RUNNING,
            degradation_factor: 0.0,
            degradation_rate: 0.0,
            degrading_sensor: None,
            anomaly_active: false,
            anomaly_sensor: None,
            anomaly_magnitude: 0.0,
            anomaly_end: None,
        }
    }

    fn update(&mut self, dt: f32, rng: &mut impl Rng) {
        self.operating_hours += dt / 3600.0;

        // Update degradation
        if self.degrading_sensor.is_some() {
            self.degradation_factor += self.degradation_rate * dt;
            self.health_score = (100.0 - self.degradation_factor * 50.0).max(0.0);

            // Update status based on health
            self.status = if self.health_score > 80.0 {
                MachineState::RUNNING
            } else if self.health_score > 50.0 {
                MachineState::MACHINE_WARNING
            } else {
                MachineState::MACHINE_CRITICAL
            };
        }

        // Check anomaly timeout
        if let Some(end_time) = self.anomaly_end {
            if Instant::now() > end_time {
                self.anomaly_active = false;
                self.anomaly_sensor = None;
                self.anomaly_end = None;
            }
        }

        // Random events
        if rng.gen_bool(0.001) && self.degrading_sensor.is_none() {
            // Start degradation on random sensor
            let sensor_idx = rng.gen_range(0..self.sensors.len());
            self.degrading_sensor = Some(self.sensors[sensor_idx].sensor_id);
            self.degradation_rate = rng.gen_range(0.01..0.05);
            info!("{}: Starting degradation on sensor {}", self.name, self.sensors[sensor_idx].sensor_id);
        }

        if rng.gen_bool(0.002) && !self.anomaly_active {
            // Trigger anomaly spike
            let sensor_idx = rng.gen_range(0..self.sensors.len());
            self.anomaly_active = true;
            self.anomaly_sensor = Some(self.sensors[sensor_idx].sensor_id);
            self.anomaly_magnitude = rng.gen_range(0.3..0.8);
            self.anomaly_end = Some(Instant::now() + Duration::from_secs(rng.gen_range(5..30)));
            info!("{}: Anomaly spike on sensor {}", self.name, self.sensors[sensor_idx].sensor_id);
        }
    }

    fn get_sensor_value(&self, sensor: &SensorConfig, rng: &mut impl Rng) -> f32 {
        let mut value = sensor.nominal + rng.gen_range(-sensor.noise..sensor.noise);

        // Apply degradation
        if self.degrading_sensor == Some(sensor.sensor_id) {
            let range = sensor.max_threshold - sensor.nominal;
            value += self.degradation_factor * range;
        }

        // Apply anomaly
        if self.anomaly_active && self.anomaly_sensor == Some(sensor.sensor_id) {
            let range = sensor.max_threshold - sensor.nominal;
            value += self.anomaly_magnitude * range;
        }

        value
    }
}

fn create_machines() -> Vec<Machine> {
    vec![
        Machine::new(1, "Pump Station A", vec![
            SensorConfig { sensor_id: 101, sensor_type: SensorType::TEMPERATURE, nominal: 45.0, noise: 2.0, min_threshold: 20.0, max_threshold: 80.0 },
            SensorConfig { sensor_id: 102, sensor_type: SensorType::VIBRATION, nominal: 2.5, noise: 0.5, min_threshold: 0.0, max_threshold: 10.0 },
            SensorConfig { sensor_id: 103, sensor_type: SensorType::PRESSURE, nominal: 4.5, noise: 0.2, min_threshold: 2.0, max_threshold: 8.0 },
            SensorConfig { sensor_id: 104, sensor_type: SensorType::CURRENT, nominal: 15.0, noise: 1.0, min_threshold: 0.0, max_threshold: 25.0 },
        ]),
        Machine::new(2, "Compressor B", vec![
            SensorConfig { sensor_id: 201, sensor_type: SensorType::TEMPERATURE, nominal: 65.0, noise: 3.0, min_threshold: 30.0, max_threshold: 100.0 },
            SensorConfig { sensor_id: 202, sensor_type: SensorType::VIBRATION, nominal: 4.0, noise: 0.8, min_threshold: 0.0, max_threshold: 15.0 },
            SensorConfig { sensor_id: 203, sensor_type: SensorType::PRESSURE, nominal: 12.0, noise: 0.5, min_threshold: 5.0, max_threshold: 20.0 },
            SensorConfig { sensor_id: 204, sensor_type: SensorType::RPM, nominal: 3000.0, noise: 50.0, min_threshold: 1000.0, max_threshold: 4000.0 },
        ]),
        Machine::new(3, "Motor Drive C", vec![
            SensorConfig { sensor_id: 301, sensor_type: SensorType::TEMPERATURE, nominal: 55.0, noise: 2.5, min_threshold: 25.0, max_threshold: 90.0 },
            SensorConfig { sensor_id: 302, sensor_type: SensorType::VIBRATION, nominal: 1.8, noise: 0.3, min_threshold: 0.0, max_threshold: 8.0 },
            SensorConfig { sensor_id: 303, sensor_type: SensorType::CURRENT, nominal: 45.0, noise: 3.0, min_threshold: 0.0, max_threshold: 80.0 },
            SensorConfig { sensor_id: 304, sensor_type: SensorType::RPM, nominal: 1750.0, noise: 20.0, min_threshold: 500.0, max_threshold: 2000.0 },
        ]),
        Machine::new(4, "Heat Exchanger D", vec![
            SensorConfig { sensor_id: 401, sensor_type: SensorType::TEMPERATURE, nominal: 75.0, noise: 4.0, min_threshold: 40.0, max_threshold: 120.0 },
            SensorConfig { sensor_id: 402, sensor_type: SensorType::PRESSURE, nominal: 3.0, noise: 0.15, min_threshold: 1.0, max_threshold: 6.0 },
            SensorConfig { sensor_id: 403, sensor_type: SensorType::FLOW_RATE, nominal: 150.0, noise: 10.0, min_threshold: 50.0, max_threshold: 250.0 },
        ]),
    ]
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info")
    ).init();

    info!("==========================================");
    info!("  Predictive Maintenance Simulator");
    info!("  HDDS Industrial IoT Demo");
    info!("==========================================");

    // Create DDS participant
    let participant = Participant::builder("pm_simulator")
        .domain_id(0)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::best_effort();

    // Create writers
    let sensor_writer = participant.topic::<SensorReading>(topics::SENSOR_READING)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let machine_writer = participant.topic::<MachineStatus>(topics::MACHINE_STATUS)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let alert_writer = participant.topic::<Alert>(topics::ALERT)?
        .writer()
        .qos(qos.clone())
        .build()?;
    let prediction_writer = participant.topic::<MaintenancePrediction>(topics::PREDICTION)?
        .writer()
        .qos(qos)
        .build()?;

    info!("DDS participant ready");

    // Initialize machines
    let mut machines = create_machines();
    let mut rng = rand::thread_rng();
    let mut next_alert_id: u32 = 1;
    let mut next_prediction_id: u32 = 1;
    let mut last_tick = Instant::now();
    let mut last_prediction_check = Instant::now();
    let mut active_alerts: HashMap<u32, Instant> = HashMap::new(); // sensor_id -> last alert time

    info!("Simulating {} machines", machines.len());
    for machine in &machines {
        info!("  {} ({} sensors)", machine.name, machine.sensors.len());
    }

    // Publish initial machine status
    for machine in &machines {
        machine_writer.write(&MachineStatus {
            machine_id: machine.machine_id,
            name: machine.name.clone(),
            status: machine.status,
            health_score: machine.health_score as u8,
            operating_hours: machine.operating_hours,
            sensor_count: machine.sensors.len() as u8,
            timestamp_us: timestamp_us(),
        })?;
    }

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        for machine in &mut machines {
            machine.update(dt, &mut rng);

            // Publish sensor readings
            for sensor in &machine.sensors {
                let value = machine.get_sensor_value(sensor, &mut rng);

                sensor_writer.write(&SensorReading {
                    sensor_id: sensor.sensor_id,
                    machine_id: machine.machine_id,
                    sensor_type: sensor.sensor_type,
                    value,
                    min_threshold: sensor.min_threshold,
                    max_threshold: sensor.max_threshold,
                    timestamp_us: timestamp_us(),
                })?;

                // Check for threshold violations
                let threshold_exceeded = value > sensor.max_threshold * 0.9 || value < sensor.min_threshold * 1.1;
                let can_alert = active_alerts.get(&sensor.sensor_id)
                    .map(|t| now.duration_since(*t).as_secs() > 30)
                    .unwrap_or(true);

                if threshold_exceeded && can_alert {
                    let severity = if value > sensor.max_threshold || value < sensor.min_threshold {
                        AlertSeverity::ALERT_CRITICAL
                    } else {
                        AlertSeverity::ALERT_WARNING
                    };

                    let (alert_type, threshold) = if value > sensor.max_threshold * 0.9 {
                        ("HIGH_VALUE", sensor.max_threshold)
                    } else {
                        ("LOW_VALUE", sensor.min_threshold)
                    };

                    alert_writer.write(&Alert {
                        alert_id: next_alert_id,
                        machine_id: machine.machine_id,
                        sensor_id: sensor.sensor_id,
                        severity,
                        alert_type: alert_type.to_string(),
                        message: format!("{} {} {} ({:.1} {})",
                            machine.name,
                            sensor_type_name(sensor.sensor_type),
                            if value > threshold { "high" } else { "low" },
                            value,
                            sensor_type_unit(sensor.sensor_type)
                        ),
                        value,
                        threshold,
                        timestamp_us: timestamp_us(),
                    })?;

                    next_alert_id += 1;
                    active_alerts.insert(sensor.sensor_id, now);
                    info!("Alert: {} sensor {} = {:.1}", machine.name, sensor.sensor_id, value);
                }
            }

            // Publish machine status
            machine_writer.write(&MachineStatus {
                machine_id: machine.machine_id,
                name: machine.name.clone(),
                status: machine.status,
                health_score: machine.health_score as u8,
                operating_hours: machine.operating_hours,
                sensor_count: machine.sensors.len() as u8,
                timestamp_us: timestamp_us(),
            })?;
        }

        // Generate predictions periodically
        if now.duration_since(last_prediction_check).as_secs() > 10 {
            last_prediction_check = now;

            for machine in &machines {
                if machine.degrading_sensor.is_some() && machine.health_score < 90.0 {
                    // Calculate time to failure based on degradation rate
                    let remaining_health = machine.health_score;
                    let hours_to_failure = (remaining_health / (machine.degradation_rate * 3600.0)).max(1.0);

                    let (failure_type, recommendation) = if machine.degradation_factor > 0.5 {
                        ("BEARING_WEAR", "Schedule bearing replacement within 24 hours")
                    } else if machine.health_score < 70.0 {
                        ("MOTOR_DEGRADATION", "Reduce load and plan maintenance")
                    } else {
                        ("EARLY_WEAR", "Monitor closely, plan inspection")
                    };

                    prediction_writer.write(&MaintenancePrediction {
                        prediction_id: next_prediction_id,
                        machine_id: machine.machine_id,
                        failure_type: failure_type.to_string(),
                        hours_to_failure,
                        confidence: (80.0 + machine.degradation_factor * 15.0).min(99.0) as u8,
                        recommendation: recommendation.to_string(),
                        timestamp_us: timestamp_us(),
                    })?;

                    next_prediction_id += 1;
                    info!("Prediction: {} - {} in {:.0}h ({}% confidence)",
                        machine.name, failure_type, hours_to_failure,
                        (80.0 + machine.degradation_factor * 15.0).min(99.0) as u8);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(SIM_TICK_MS)).await;
    }
}
