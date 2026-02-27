// CONCEPT DEMO - Simulated data for showcase purposes
//! Fort Knox Security - Multi-zone Sensor Simulator
//!
//! Simulates 20 sensors across 5 security zones, publishing SensorReading
//! at 2 Hz. Generates occasional SecurityAlerts, AccessEvents, VaultStatus
//! updates, and AuditLogEntries to exercise the full DDS Security pipeline.

use hdds::{Participant, QoS, TransportMode};
use knox_types::*;
use rand::Rng;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Sensor definitions (20 sensors across 5 zones)
// ---------------------------------------------------------------------------

struct SensorDef {
    id: &'static str,
    sensor_type: SensorType,
    zone: Zone,
    baseline: f64,
    noise: f64,
}

const SENSORS: &[SensorDef] = &[
    // Zone 0: Public Lobby (4 sensors)
    SensorDef { id: "CAM-LOBBY-01", sensor_type: SensorType::CAMERA, zone: Zone::PUBLIC_LOBBY, baseline: 0.7, noise: 0.1 },
    SensorDef { id: "CAM-LOBBY-02", sensor_type: SensorType::CAMERA, zone: Zone::PUBLIC_LOBBY, baseline: 0.8, noise: 0.1 },
    SensorDef { id: "MOT-LOBBY-01", sensor_type: SensorType::MOTION, zone: Zone::PUBLIC_LOBBY, baseline: 0.3, noise: 0.2 },
    SensorDef { id: "DOOR-LOBBY-01", sensor_type: SensorType::DOOR, zone: Zone::PUBLIC_LOBBY, baseline: 0.0, noise: 0.05 },
    // Zone 1: Teller Area (4 sensors)
    SensorDef { id: "CAM-TELLER-01", sensor_type: SensorType::CAMERA, zone: Zone::TELLER_AREA, baseline: 0.9, noise: 0.05 },
    SensorDef { id: "MOT-TELLER-01", sensor_type: SensorType::MOTION, zone: Zone::TELLER_AREA, baseline: 0.2, noise: 0.15 },
    SensorDef { id: "DOOR-TELLER-01", sensor_type: SensorType::DOOR, zone: Zone::TELLER_AREA, baseline: 0.0, noise: 0.05 },
    SensorDef { id: "THERM-TELLER-01", sensor_type: SensorType::THERMAL, zone: Zone::TELLER_AREA, baseline: 22.0, noise: 1.0 },
    // Zone 2: Vault Corridor (4 sensors)
    SensorDef { id: "CAM-CORR-01", sensor_type: SensorType::CAMERA, zone: Zone::VAULT_CORRIDOR, baseline: 0.95, noise: 0.02 },
    SensorDef { id: "MOT-CORR-01", sensor_type: SensorType::MOTION, zone: Zone::VAULT_CORRIDOR, baseline: 0.05, noise: 0.05 },
    SensorDef { id: "VIB-CORR-01", sensor_type: SensorType::VIBRATION, zone: Zone::VAULT_CORRIDOR, baseline: 0.01, noise: 0.005 },
    SensorDef { id: "DOOR-CORR-01", sensor_type: SensorType::DOOR, zone: Zone::VAULT_CORRIDOR, baseline: 0.0, noise: 0.02 },
    // Zone 3: Vault Room (4 sensors)
    SensorDef { id: "CAM-VAULT-01", sensor_type: SensorType::CAMERA, zone: Zone::VAULT_ROOM, baseline: 0.99, noise: 0.005 },
    SensorDef { id: "VIB-VAULT-01", sensor_type: SensorType::VIBRATION, zone: Zone::VAULT_ROOM, baseline: 0.005, noise: 0.002 },
    SensorDef { id: "WEIGHT-VAULT-01", sensor_type: SensorType::WEIGHT, zone: Zone::VAULT_ROOM, baseline: 5000.0, noise: 0.5 },
    SensorDef { id: "THERM-VAULT-01", sensor_type: SensorType::THERMAL, zone: Zone::VAULT_ROOM, baseline: 18.0, noise: 0.3 },
    // Zone 4: Server Room (4 sensors)
    SensorDef { id: "CAM-SERVER-01", sensor_type: SensorType::CAMERA, zone: Zone::SERVER_ROOM, baseline: 0.95, noise: 0.02 },
    SensorDef { id: "MOT-SERVER-01", sensor_type: SensorType::MOTION, zone: Zone::SERVER_ROOM, baseline: 0.02, noise: 0.03 },
    SensorDef { id: "THERM-SERVER-01", sensor_type: SensorType::THERMAL, zone: Zone::SERVER_ROOM, baseline: 20.0, noise: 0.5 },
    SensorDef { id: "DOOR-SERVER-01", sensor_type: SensorType::DOOR, zone: Zone::SERVER_ROOM, baseline: 0.0, noise: 0.02 },
];

// ---------------------------------------------------------------------------
// Personnel for access events
// ---------------------------------------------------------------------------

struct PersonDef {
    id: &'static str,
    name: &'static str,
    clearance: Zone, // max zone they can access
}

const PERSONNEL: &[PersonDef] = &[
    PersonDef { id: "EMP-001", name: "Alice Johnson", clearance: Zone::PUBLIC_LOBBY },
    PersonDef { id: "EMP-002", name: "Bob Williams", clearance: Zone::TELLER_AREA },
    PersonDef { id: "EMP-003", name: "Carol Martinez", clearance: Zone::VAULT_CORRIDOR },
    PersonDef { id: "SEC-001", name: "David Chen", clearance: Zone::VAULT_ROOM },
    PersonDef { id: "SEC-002", name: "Eva Kowalski", clearance: Zone::VAULT_ROOM },
    PersonDef { id: "ADM-001", name: "Frank OBrien", clearance: Zone::SERVER_ROOM },
    PersonDef { id: "ADM-002", name: "Grace Nakamura", clearance: Zone::SERVER_ROOM },
    PersonDef { id: "VIS-001", name: "Hank Visitor", clearance: Zone::PUBLIC_LOBBY },
];

// ---------------------------------------------------------------------------
// Zone ordering helpers (enums are not Ord by default)
// ---------------------------------------------------------------------------

/// All zones in order, for indexed access.
const ALL_ZONES: [Zone; ZONE_COUNT] = [
    Zone::PUBLIC_LOBBY,
    Zone::TELLER_AREA,
    Zone::VAULT_CORRIDOR,
    Zone::VAULT_ROOM,
    Zone::SERVER_ROOM,
];

fn zone_index(z: Zone) -> usize {
    z as u32 as usize
}

fn zone_from_index(i: usize) -> Zone {
    ALL_ZONES[i % ZONE_COUNT]
}

// ---------------------------------------------------------------------------
// Alert scenario templates
// ---------------------------------------------------------------------------

struct AlertScenario {
    zone: Zone,
    severity: Severity,
    description: &'static str,
}

const ALERT_SCENARIOS: &[AlertScenario] = &[
    AlertScenario { zone: Zone::PUBLIC_LOBBY, severity: Severity::INFO, description: "Lobby occupancy above normal threshold" },
    AlertScenario { zone: Zone::PUBLIC_LOBBY, severity: Severity::WARNING, description: "Unidentified individual loitering near entrance" },
    AlertScenario { zone: Zone::TELLER_AREA, severity: Severity::INFO, description: "Cash drawer opened outside transaction" },
    AlertScenario { zone: Zone::TELLER_AREA, severity: Severity::WARNING, description: "Camera obscured - teller station 3" },
    AlertScenario { zone: Zone::VAULT_CORRIDOR, severity: Severity::WARNING, description: "Motion detected outside business hours" },
    AlertScenario { zone: Zone::VAULT_CORRIDOR, severity: Severity::CRITICAL, description: "Vibration anomaly detected - wall section B" },
    AlertScenario { zone: Zone::VAULT_ROOM, severity: Severity::CRITICAL, description: "Weight discrepancy detected in vault" },
    AlertScenario { zone: Zone::VAULT_ROOM, severity: Severity::BREACH, description: "Vault door forced open - tamper detected" },
    AlertScenario { zone: Zone::SERVER_ROOM, severity: Severity::WARNING, description: "Server room temperature rising" },
    AlertScenario { zone: Zone::SERVER_ROOM, severity: Severity::CRITICAL, description: "Unauthorized access attempt - server rack" },
];

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    tracing::info!("=============================================");
    tracing::info!("  Fort Knox Security - Sensor Simulator");
    tracing::info!("  HDDS DDS Security Concept Demo");
    tracing::info!("=============================================");

    // Create DDS participant
    let participant = Participant::builder("knox_sensors")
        .domain_id(42)
        .with_transport(TransportMode::UdpMulticast)
        .build()?;

    let qos = QoS::default();

    // Create writers for each topic
    let sensor_writer = participant.create_writer::<SensorReading>(
        topics::SENSOR_READING, qos.clone(),
    )?;
    let alert_writer = participant.create_writer::<SecurityAlert>(
        topics::SECURITY_ALERT, qos.clone(),
    )?;
    let access_writer = participant.create_writer::<AccessEvent>(
        topics::ACCESS_EVENT, qos.clone(),
    )?;
    let vault_writer = participant.create_writer::<VaultStatus>(
        topics::VAULT_STATUS, qos.clone(),
    )?;
    let audit_writer = participant.create_writer::<AuditLogEntry>(
        topics::AUDIT_LOG, qos,
    )?;

    tracing::info!("DDS participant created on domain 42");
    tracing::info!("Publishing {} sensors across {} zones", SENSORS.len(), ZONE_COUNT);

    let mut rng = rand::thread_rng();
    let mut next_alert_id: u64 = 1;
    let mut next_event_id: u64 = 1;
    let mut audit_seq: i64 = 1;

    // Track personnel locations for access events
    let mut person_locations: HashMap<&str, Zone> = HashMap::new();
    for person in PERSONNEL {
        person_locations.insert(person.id, Zone::PUBLIC_LOBBY);
    }

    // Vault state
    let mut vault_door_sealed = true;
    let mut vault_weight_kg: f64 = 5000.0;
    let expected_vault_weight: f64 = 5000.0;

    let mut last_alert = Instant::now();
    let mut last_access_event = Instant::now();
    let mut last_vault_update = Instant::now();

    // Publish initial audit entry
    audit_writer.write(&AuditLogEntry {
        sequence_number: audit_seq,
        source_entity: "knox-sensors".to_string(),
        action: "SYSTEM_START".to_string(),
        details: "Sensor simulator initialized with 20 sensors across 5 zones".to_string(),
        timestamp_ns: timestamp_ns(),
    })?;
    audit_seq += 1;

    tracing::info!("Entering main sensor loop (2 Hz per sensor)...");

    loop {
        let now = Instant::now();

        // Publish sensor readings at 2 Hz
        for sensor_def in SENSORS {
            let tamper = rng.gen_bool(0.001); // 0.1% tamper chance
            let value = sensor_def.baseline
                + rng.gen_range(-sensor_def.noise..sensor_def.noise);

            let reading = SensorReading {
                sensor_id: sensor_def.id.to_string(),
                sensor_type: sensor_def.sensor_type,
                zone: sensor_def.zone,
                value,
                confidence: 0.85 + rng.gen_range(0.0..0.15),
                timestamp_ns: timestamp_ns(),
                tamper_detected: tamper,
            };

            sensor_writer.write(&reading)?;

            if tamper {
                tracing::warn!(
                    "TAMPER detected on sensor {} in {}",
                    sensor_def.id, sensor_def.zone.name()
                );

                // Auto-generate alert for tamper
                let alert = SecurityAlert {
                    alert_id: format!("ALT-{:06}", next_alert_id),
                    zone: sensor_def.zone,
                    severity: Severity::CRITICAL,
                    description: format!(
                        "Tamper detected on sensor {} ({})",
                        sensor_def.id,
                        sensor_def.sensor_type.name()
                    ),
                    timestamp_ns: timestamp_ns(),
                    acknowledged: false,
                };
                alert_writer.write(&alert)?;
                next_alert_id += 1;

                audit_writer.write(&AuditLogEntry {
                    sequence_number: audit_seq,
                    source_entity: sensor_def.id.to_string(),
                    action: "TAMPER_DETECTED".to_string(),
                    details: format!(
                        "Sensor tamper in zone {} ({})",
                        sensor_def.zone.name(),
                        sensor_def.sensor_type.name()
                    ),
                    timestamp_ns: timestamp_ns(),
                })?;
                audit_seq += 1;
            }
        }

        // Generate random alerts every 5-15 seconds
        if now.duration_since(last_alert).as_secs() >= rng.gen_range(5..15) {
            let scenario = &ALERT_SCENARIOS[rng.gen_range(0..ALERT_SCENARIOS.len())];

            let alert = SecurityAlert {
                alert_id: format!("ALT-{:06}", next_alert_id),
                zone: scenario.zone,
                severity: scenario.severity,
                description: scenario.description.to_string(),
                timestamp_ns: timestamp_ns(),
                acknowledged: false,
            };

            tracing::info!(
                "[ALERT] {} | {} | {}",
                scenario.severity.name(),
                scenario.zone.name(),
                scenario.description
            );

            alert_writer.write(&alert)?;
            next_alert_id += 1;
            last_alert = now;

            audit_writer.write(&AuditLogEntry {
                sequence_number: audit_seq,
                source_entity: "alert-system".to_string(),
                action: format!("ALERT_{}", scenario.severity.name()),
                details: scenario.description.to_string(),
                timestamp_ns: timestamp_ns(),
            })?;
            audit_seq += 1;
        }

        // Generate access events every 3-8 seconds
        if now.duration_since(last_access_event).as_secs() >= rng.gen_range(3..8) {
            let person = &PERSONNEL[rng.gen_range(0..PERSONNEL.len())];
            let current_zone = *person_locations.get(person.id).unwrap_or(&Zone::PUBLIC_LOBBY);
            let current_idx = zone_index(current_zone);

            // Try to move to an adjacent zone (or random zone)
            let target_idx = if rng.gen_bool(0.7) {
                // Adjacent move
                if rng.gen_bool(0.5) && current_idx < ZONE_COUNT - 1 {
                    current_idx + 1
                } else if current_idx > 0 {
                    current_idx - 1
                } else {
                    current_idx + 1
                }
            } else {
                // Random zone
                rng.gen_range(0..ZONE_COUNT)
            };
            let target_zone = zone_from_index(target_idx);

            let granted = zone_index(target_zone) <= zone_index(person.clearance);
            let reason = if granted {
                "Clearance verified".to_string()
            } else {
                format!(
                    "Insufficient clearance for {} (requires level {}+)",
                    target_zone.name(), zone_index(target_zone)
                )
            };

            let event = AccessEvent {
                event_id: format!("ACC-{:06}", next_event_id),
                person_id: person.id.to_string(),
                person_name: person.name.to_string(),
                from_zone: current_zone,
                to_zone: target_zone,
                granted,
                reason: reason.clone(),
                timestamp_ns: timestamp_ns(),
            };

            if granted {
                person_locations.insert(person.id, target_zone);
                tracing::info!(
                    "[ACCESS] GRANTED | {} ({}) | {} -> {}",
                    person.name, person.id,
                    current_zone.name(), target_zone.name()
                );
            } else {
                tracing::warn!(
                    "[ACCESS] DENIED  | {} ({}) | {} -> {} | {}",
                    person.name, person.id,
                    current_zone.name(), target_zone.name(),
                    reason
                );

                // Denied access generates a warning alert
                let alert = SecurityAlert {
                    alert_id: format!("ALT-{:06}", next_alert_id),
                    zone: target_zone,
                    severity: Severity::WARNING,
                    description: format!(
                        "Access denied: {} attempted entry to {}",
                        person.name, target_zone.name()
                    ),
                    timestamp_ns: timestamp_ns(),
                    acknowledged: false,
                };
                alert_writer.write(&alert)?;
                next_alert_id += 1;
            }

            access_writer.write(&event)?;
            next_event_id += 1;
            last_access_event = now;

            audit_writer.write(&AuditLogEntry {
                sequence_number: audit_seq,
                source_entity: person.id.to_string(),
                action: if granted { "ACCESS_GRANTED" } else { "ACCESS_DENIED" }.to_string(),
                details: format!(
                    "{} ({}) {} -> {}",
                    person.name, person.id,
                    current_zone.name(), target_zone.name()
                ),
                timestamp_ns: timestamp_ns(),
            })?;
            audit_seq += 1;
        }

        // Update vault status every 2 seconds
        if now.duration_since(last_vault_update).as_secs() >= 2 {
            // Simulate minor weight fluctuations
            vault_weight_kg += rng.gen_range(-0.3..0.3);
            let weight_anomaly = (vault_weight_kg - expected_vault_weight).abs() > 2.0;

            // Rarely toggle vault door
            if rng.gen_bool(0.02) {
                vault_door_sealed = !vault_door_sealed;
                let action = if vault_door_sealed { "VAULT_SEALED" } else { "VAULT_OPENED" };
                tracing::info!("[VAULT] Door state changed: {}", action);

                audit_writer.write(&AuditLogEntry {
                    sequence_number: audit_seq,
                    source_entity: "vault-controller".to_string(),
                    action: action.to_string(),
                    details: format!("Vault door {}", if vault_door_sealed { "sealed" } else { "opened" }),
                    timestamp_ns: timestamp_ns(),
                })?;
                audit_seq += 1;
            }

            let status = VaultStatus {
                vault_id: 1,
                door_sealed: vault_door_sealed,
                internal_temperature: 18.0 + rng.gen_range(-0.5..0.5),
                internal_humidity: 35.0 + rng.gen_range(-2.0..2.0),
                weight_kg: vault_weight_kg,
                expected_weight_kg: expected_vault_weight,
                weight_anomaly,
                last_opened_ns: if vault_door_sealed { 0 } else { timestamp_ns() },
            };

            vault_writer.write(&status)?;
            last_vault_update = now;

            if weight_anomaly {
                tracing::warn!(
                    "[VAULT] Weight anomaly: {:.1} kg (expected {:.1} kg)",
                    vault_weight_kg, expected_vault_weight
                );
            }
        }

        // 500ms sleep = 2 Hz publish rate
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
