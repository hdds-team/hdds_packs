// CONCEPT DEMO - Simulated data for showcase purposes
//
//! Fleet Services - 4 RPC services in a single binary
//!
//! Demonstrates the DDS-RPC pattern: typed request/reply dispatch
//! over standard DDS pub/sub topics.
//!
//! Services:
//!   1. VehicleService  - Register and locate fleet vehicles
//!   2. RouteService    - Compute routes (haversine distance)
//!   3. MaintenanceService - Track maintenance tickets
//!   4. AlertService    - Process fleet alerts

use fleet_types::*;
use hdds::{Participant, QoS, TransportMode};
use rand::Rng;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tracing::info;

// ============================================================================
// Shared state types
// ============================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct VehicleRecord {
    vehicle_id: String,
    plate_number: String,
    model: String,
    max_payload_kg: f64,
    latitude: f64,
    longitude: f64,
    state: VehicleState,
    speed_kmh: f64,
    last_update_ns: i64,
}

#[derive(Debug, Clone)]
struct TicketRecord {
    ticket_id: String,
    vehicle_id: String,
    description: String,
    priority: u32,
    status: TicketStatus,
    created_ns: i64,
    updated_ns: i64,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct AlertRecord {
    alert_id: String,
    vehicle_id: String,
    severity: AlertSeverity,
    message: String,
    latitude: f64,
    longitude: f64,
    acknowledged: bool,
    created_ns: i64,
}

type VehicleDb = Arc<Mutex<HashMap<String, VehicleRecord>>>;
type TicketDb = Arc<Mutex<HashMap<String, TicketRecord>>>;
type AlertDb = Arc<Mutex<Vec<AlertRecord>>>;

// ============================================================================
// Haversine distance (km)
// ============================================================================

fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let d_lat = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

// ============================================================================
// Seed initial vehicles
// ============================================================================

fn seed_vehicles(db: &VehicleDb) {
    let mut vehicles = db.lock().unwrap();
    let seed_data = [
        ("VH-001", "AB-123-CD", "Renault Kangoo", 800.0, 48.8566, 2.3522),
        ("VH-002", "EF-456-GH", "Mercedes Sprinter", 1500.0, 48.8606, 2.3376),
        ("VH-003", "IJ-789-KL", "Iveco Daily", 2000.0, 48.8738, 2.2950),
        ("VH-004", "MN-012-OP", "Peugeot Expert", 1200.0, 48.8462, 2.3464),
        ("VH-005", "QR-345-ST", "Citroen Jumpy", 1100.0, 48.8530, 2.3499),
        ("VH-006", "UV-678-WX", "Fiat Ducato", 1800.0, 48.8650, 2.3200),
        ("VH-007", "YZ-901-AB", "VW Crafter", 2200.0, 48.8700, 2.3100),
        ("VH-008", "CD-234-EF", "Ford Transit", 1600.0, 48.8400, 2.3600),
        ("VH-009", "GH-567-IJ", "Opel Movano", 1900.0, 48.8550, 2.3800),
        ("VH-010", "KL-890-MN", "Toyota ProAce", 1000.0, 48.8480, 2.3300),
    ];

    for (id, plate, model, payload, lat, lon) in seed_data {
        vehicles.insert(
            id.to_string(),
            VehicleRecord {
                vehicle_id: id.to_string(),
                plate_number: plate.to_string(),
                model: model.to_string(),
                max_payload_kg: payload,
                latitude: lat,
                longitude: lon,
                state: VehicleState::IDLE,
                speed_kmh: 0.0,
                last_update_ns: timestamp_ns(),
            },
        );
    }
}

// ============================================================================
// VehicleService thread
// ============================================================================

fn run_vehicle_service(
    participant: Arc<Participant>,
    vehicles: VehicleDb,
) {
    let qos = QoS::default();

    // Service registration writer
    let reg_writer = participant
        .create_writer::<ServiceRegistration>(topics::SERVICE_REGISTRY, qos.clone())
        .expect("Failed to create service registry writer");

    // Request readers and reply writers
    let register_reader = participant
        .create_reader::<RegisterVehicleRequest>(topics::REGISTER_VEHICLE_REQ, qos.clone())
        .expect("Failed to create RegisterVehicle request reader");
    let register_writer = participant
        .create_writer::<RegisterVehicleReply>(topics::REGISTER_VEHICLE_REP, qos.clone())
        .expect("Failed to create RegisterVehicle reply writer");

    let locate_reader = participant
        .create_reader::<LocateVehicleRequest>(topics::LOCATE_VEHICLE_REQ, qos.clone())
        .expect("Failed to create LocateVehicle request reader");
    let locate_writer = participant
        .create_writer::<LocateVehicleReply>(topics::LOCATE_VEHICLE_REP, qos.clone())
        .expect("Failed to create LocateVehicle reply writer");

    info!("VehicleService started ({} vehicles loaded)", vehicles.lock().unwrap().len());

    let mut heartbeat_counter = 0u64;

    loop {
        // Publish heartbeat every ~10s (100 iterations * 100ms sleep)
        if heartbeat_counter % 100 == 0 {
            let _ = reg_writer.write(&ServiceRegistration {
                service_name: "VehicleService".to_string(),
                service_type: "fleet.vehicle.v1".to_string(),
                endpoint: "dds://rpc/VehicleService".to_string(),
                registered_ns: timestamp_ns(),
                healthy: true,
            });
        }
        heartbeat_counter += 1;

        // Handle RegisterVehicle requests
        while let Ok(Some(req)) = register_reader.take() {
            info!("VehicleService: RegisterVehicle plate={}", req.plate_number);
            let mut db = vehicles.lock().unwrap();
            let id = format!("VH-{:03}", db.len() + 1);
            db.insert(
                id.clone(),
                VehicleRecord {
                    vehicle_id: id.clone(),
                    plate_number: req.plate_number.clone(),
                    model: req.model.clone(),
                    max_payload_kg: req.max_payload_kg,
                    latitude: req.home_lat,
                    longitude: req.home_lon,
                    state: VehicleState::IDLE,
                    speed_kmh: 0.0,
                    last_update_ns: timestamp_ns(),
                },
            );
            let _ = register_writer.write(&RegisterVehicleReply {
                vehicle_id: id,
                success: true,
                error: String::new(),
            });
        }

        // Handle LocateVehicle requests
        while let Ok(Some(req)) = locate_reader.take() {
            info!("VehicleService: LocateVehicle id={}", req.vehicle_id);
            let db = vehicles.lock().unwrap();
            let reply = if let Some(v) = db.get(&req.vehicle_id) {
                LocateVehicleReply {
                    vehicle_id: v.vehicle_id.clone(),
                    latitude: v.latitude,
                    longitude: v.longitude,
                    state: v.state,
                    speed_kmh: v.speed_kmh,
                    last_update_ns: v.last_update_ns,
                }
            } else {
                LocateVehicleReply {
                    vehicle_id: req.vehicle_id,
                    latitude: 0.0,
                    longitude: 0.0,
                    state: VehicleState::IDLE,
                    speed_kmh: 0.0,
                    last_update_ns: 0,
                }
            };
            let _ = locate_writer.write(&reply);
        }

        // Simulate vehicle movement
        {
            let mut db = vehicles.lock().unwrap();
            let mut rng = rand::thread_rng();
            for vehicle in db.values_mut() {
                if vehicle.state == VehicleState::EN_ROUTE {
                    // Drift position slightly
                    vehicle.latitude += rng.gen_range(-0.001..0.001);
                    vehicle.longitude += rng.gen_range(-0.001..0.001);
                    vehicle.speed_kmh = rng.gen_range(20.0..80.0);
                    vehicle.last_update_ns = timestamp_ns();
                }
                // Randomly change state
                if rng.gen_range(0u32..200) == 0 {
                    vehicle.state = match rng.gen_range(0u32..5) {
                        0 => VehicleState::IDLE,
                        1 => VehicleState::EN_ROUTE,
                        2 => VehicleState::LOADING,
                        3 => VehicleState::MAINTENANCE,
                        4 => VehicleState::EMERGENCY,
                        _ => unreachable!(),
                    };
                    if vehicle.state == VehicleState::IDLE {
                        vehicle.speed_kmh = 0.0;
                    }
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ============================================================================
// RouteService thread
// ============================================================================

fn run_route_service(participant: Arc<Participant>) {
    let qos = QoS::default();

    let reg_writer = participant
        .create_writer::<ServiceRegistration>(topics::SERVICE_REGISTRY, qos.clone())
        .expect("Failed to create service registry writer");

    let route_reader = participant
        .create_reader::<ComputeRouteRequest>(topics::COMPUTE_ROUTE_REQ, qos.clone())
        .expect("Failed to create ComputeRoute request reader");
    let route_writer = participant
        .create_writer::<ComputeRouteReply>(topics::COMPUTE_ROUTE_REP, qos.clone())
        .expect("Failed to create ComputeRoute reply writer");

    info!("RouteService started");

    let mut heartbeat_counter = 0u64;
    let mut route_counter = 0u64;

    loop {
        if heartbeat_counter % 100 == 0 {
            let _ = reg_writer.write(&ServiceRegistration {
                service_name: "RouteService".to_string(),
                service_type: "fleet.route.v1".to_string(),
                endpoint: "dds://rpc/RouteService".to_string(),
                registered_ns: timestamp_ns(),
                healthy: true,
            });
        }
        heartbeat_counter += 1;

        while let Ok(Some(req)) = route_reader.take() {
            info!(
                "RouteService: ComputeRoute ({:.4},{:.4}) -> ({:.4},{:.4})",
                req.origin_lat, req.origin_lon, req.dest_lat, req.dest_lon
            );

            let distance = haversine_km(req.origin_lat, req.origin_lon, req.dest_lat, req.dest_lon);
            let base_minutes = distance / 60.0 * 60.0; // ~60 km/h average
            let toll_factor = if req.avoid_tolls { 1.3 } else { 1.0 };
            let estimated = base_minutes * toll_factor;

            route_counter += 1;
            let _ = route_writer.write(&ComputeRouteReply {
                route_id: format!("RT-{:06}", route_counter),
                distance_km: distance,
                estimated_minutes: estimated,
                success: true,
            });
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ============================================================================
// MaintenanceService thread
// ============================================================================

fn run_maintenance_service(
    participant: Arc<Participant>,
    tickets: TicketDb,
) {
    let qos = QoS::default();

    let reg_writer = participant
        .create_writer::<ServiceRegistration>(topics::SERVICE_REGISTRY, qos.clone())
        .expect("Failed to create service registry writer");

    let create_reader = participant
        .create_reader::<CreateTicketRequest>(topics::CREATE_TICKET_REQ, qos.clone())
        .expect("Failed to create CreateTicket request reader");
    let create_writer = participant
        .create_writer::<CreateTicketReply>(topics::CREATE_TICKET_REP, qos.clone())
        .expect("Failed to create CreateTicket reply writer");

    let status_reader = participant
        .create_reader::<GetTicketStatusRequest>(topics::GET_TICKET_STATUS_REQ, qos.clone())
        .expect("Failed to create GetTicketStatus request reader");
    let status_writer = participant
        .create_writer::<GetTicketStatusReply>(topics::GET_TICKET_STATUS_REP, qos.clone())
        .expect("Failed to create GetTicketStatus reply writer");

    info!("MaintenanceService started");

    let mut heartbeat_counter = 0u64;
    let mut ticket_counter = 0u64;

    loop {
        if heartbeat_counter % 100 == 0 {
            let _ = reg_writer.write(&ServiceRegistration {
                service_name: "MaintenanceService".to_string(),
                service_type: "fleet.maintenance.v1".to_string(),
                endpoint: "dds://rpc/MaintenanceService".to_string(),
                registered_ns: timestamp_ns(),
                healthy: true,
            });
        }
        heartbeat_counter += 1;

        // Handle CreateTicket requests
        while let Ok(Some(req)) = create_reader.take() {
            info!("MaintenanceService: CreateTicket vehicle={}", req.vehicle_id);
            ticket_counter += 1;
            let ticket_id = format!("TK-{:06}", ticket_counter);
            let now = timestamp_ns();

            let mut db = tickets.lock().unwrap();
            db.insert(
                ticket_id.clone(),
                TicketRecord {
                    ticket_id: ticket_id.clone(),
                    vehicle_id: req.vehicle_id.clone(),
                    description: req.description.clone(),
                    priority: req.priority,
                    status: TicketStatus::OPEN,
                    created_ns: now,
                    updated_ns: now,
                },
            );

            let _ = create_writer.write(&CreateTicketReply {
                ticket_id,
                success: true,
                error: String::new(),
            });
        }

        // Handle GetTicketStatus requests
        while let Ok(Some(req)) = status_reader.take() {
            info!("MaintenanceService: GetTicketStatus id={}", req.ticket_id);
            let db = tickets.lock().unwrap();
            let reply = if let Some(t) = db.get(&req.ticket_id) {
                GetTicketStatusReply {
                    ticket_id: t.ticket_id.clone(),
                    vehicle_id: t.vehicle_id.clone(),
                    description: t.description.clone(),
                    priority: t.priority,
                    status: t.status,
                    created_ns: t.created_ns,
                    updated_ns: t.updated_ns,
                }
            } else {
                GetTicketStatusReply {
                    ticket_id: req.ticket_id,
                    vehicle_id: String::new(),
                    description: "Not found".to_string(),
                    priority: 0,
                    status: TicketStatus::OPEN,
                    created_ns: 0,
                    updated_ns: 0,
                }
            };
            let _ = status_writer.write(&reply);
        }

        // Simulate ticket state progression
        {
            let mut db = tickets.lock().unwrap();
            let mut rng = rand::thread_rng();
            for ticket in db.values_mut() {
                if rng.gen_range(0u32..50) == 0 {
                    ticket.status = match ticket.status {
                        TicketStatus::OPEN => TicketStatus::IN_PROGRESS,
                        TicketStatus::IN_PROGRESS => TicketStatus::RESOLVED,
                        TicketStatus::RESOLVED => TicketStatus::CLOSED,
                        TicketStatus::CLOSED => TicketStatus::CLOSED,
                    };
                    ticket.updated_ns = timestamp_ns();
                }
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ============================================================================
// AlertService thread
// ============================================================================

fn run_alert_service(
    participant: Arc<Participant>,
    alerts: AlertDb,
) {
    let qos = QoS::default();

    let reg_writer = participant
        .create_writer::<ServiceRegistration>(topics::SERVICE_REGISTRY, qos.clone())
        .expect("Failed to create service registry writer");

    let alert_reader = participant
        .create_reader::<RaiseAlertRequest>(topics::RAISE_ALERT_REQ, qos.clone())
        .expect("Failed to create RaiseAlert request reader");
    let alert_writer = participant
        .create_writer::<RaiseAlertReply>(topics::RAISE_ALERT_REP, qos.clone())
        .expect("Failed to create RaiseAlert reply writer");

    info!("AlertService started");

    let mut heartbeat_counter = 0u64;
    let mut alert_counter = 0u64;

    loop {
        if heartbeat_counter % 100 == 0 {
            let _ = reg_writer.write(&ServiceRegistration {
                service_name: "AlertService".to_string(),
                service_type: "fleet.alert.v1".to_string(),
                endpoint: "dds://rpc/AlertService".to_string(),
                registered_ns: timestamp_ns(),
                healthy: true,
            });
        }
        heartbeat_counter += 1;

        while let Ok(Some(req)) = alert_reader.take() {
            info!(
                "AlertService: RaiseAlert vehicle={} severity={:?}",
                req.vehicle_id, req.severity
            );
            alert_counter += 1;
            let alert_id = format!("AL-{:06}", alert_counter);

            let mut db = alerts.lock().unwrap();
            db.push(AlertRecord {
                alert_id: alert_id.clone(),
                vehicle_id: req.vehicle_id,
                severity: req.severity,
                message: req.message,
                latitude: req.latitude,
                longitude: req.longitude,
                acknowledged: false,
                created_ns: timestamp_ns(),
            });

            let _ = alert_writer.write(&RaiseAlertReply {
                alert_id,
                acknowledged: false,
                error: String::new(),
            });
        }

        // Auto-acknowledge old alerts
        {
            let mut db = alerts.lock().unwrap();
            let now = timestamp_ns();
            for alert in db.iter_mut() {
                // Auto-ack after ~30s
                if !alert.acknowledged && (now - alert.created_ns) > 30_000_000_000 {
                    alert.acknowledged = true;
                }
            }
            // Remove very old acknowledged alerts
            db.retain(|a| !a.acknowledged || (now - a.created_ns) < 120_000_000_000);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }
}

// ============================================================================
// Main: spawn all 4 services
// ============================================================================

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Fleet Service Mesh starting...");
    info!("  - VehicleService");
    info!("  - RouteService");
    info!("  - MaintenanceService");
    info!("  - AlertService");

    // Create DDS participant
    // build() returns Arc<Participant>
    let participant = Participant::builder("fleet_services")
        .domain_id(42)
        .with_transport(TransportMode::UdpMulticast)
        .build()
        .expect("Failed to create DDS participant");

    // Shared state
    let vehicles: VehicleDb = Arc::new(Mutex::new(HashMap::new()));
    let tickets: TicketDb = Arc::new(Mutex::new(HashMap::new()));
    let alerts: AlertDb = Arc::new(Mutex::new(Vec::new()));

    // Seed initial vehicle data
    seed_vehicles(&vehicles);
    info!("Seeded {} vehicles", vehicles.lock().unwrap().len());

    // Spawn service threads
    let p1 = Arc::clone(&participant);
    let v1 = Arc::clone(&vehicles);
    let t1 = std::thread::Builder::new()
        .name("vehicle-service".to_string())
        .spawn(move || run_vehicle_service(p1, v1))
        .expect("Failed to spawn VehicleService thread");

    let p2 = Arc::clone(&participant);
    let t2 = std::thread::Builder::new()
        .name("route-service".to_string())
        .spawn(move || run_route_service(p2))
        .expect("Failed to spawn RouteService thread");

    let p3 = Arc::clone(&participant);
    let tk = Arc::clone(&tickets);
    let t3 = std::thread::Builder::new()
        .name("maintenance-service".to_string())
        .spawn(move || run_maintenance_service(p3, tk))
        .expect("Failed to spawn MaintenanceService thread");

    let p4 = Arc::clone(&participant);
    let al = Arc::clone(&alerts);
    let t4 = std::thread::Builder::new()
        .name("alert-service".to_string())
        .spawn(move || run_alert_service(p4, al))
        .expect("Failed to spawn AlertService thread");

    info!("All 4 services running. Press Ctrl+C to stop.");

    // Wait for all threads (they run forever)
    let _ = t1.join();
    let _ = t2.join();
    let _ = t3.join();
    let _ = t4.join();
}
