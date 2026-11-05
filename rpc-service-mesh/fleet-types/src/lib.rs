// RPC Service Mesh Fleet Types
//
// Generated from IDL via: hddsgen gen rust idl/service_mesh.idl -o fleet-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]
#![allow(non_camel_case_types)]

include!("generated.rs");

// ============================================================================
// Enum display helpers
// ============================================================================

impl VehicleState {
    /// Human-readable name for this vehicle state.
    pub fn name(self) -> &'static str {
        match self {
            Self::IDLE => "Idle",
            Self::EN_ROUTE => "En Route",
            Self::LOADING => "Loading",
            Self::MAINTENANCE => "Maintenance",
            Self::VEHICLE_EMERGENCY => "Emergency",
        }
    }
}

impl TicketStatus {
    /// Human-readable name for this ticket status.
    pub fn name(self) -> &'static str {
        match self {
            Self::OPEN => "Open",
            Self::IN_PROGRESS => "In Progress",
            Self::RESOLVED => "Resolved",
            Self::CLOSED => "Closed",
        }
    }
}

impl AlertSeverity {
    /// Human-readable name for this alert severity.
    pub fn name(self) -> &'static str {
        match self {
            Self::INFO => "Info",
            Self::WARNING => "Warning",
            Self::CRITICAL => "Critical",
            Self::ALERT_EMERGENCY => "Emergency",
        }
    }
}

// ============================================================================
// Topic name constants
// ============================================================================

pub mod topics {
    // Service registry
    pub const SERVICE_REGISTRY: &str = "rpc/ServiceRegistry";

    // VehicleService
    pub const REGISTER_VEHICLE_REQ: &str = "rpc/VehicleService/RegisterVehicle/Request";
    pub const REGISTER_VEHICLE_REP: &str = "rpc/VehicleService/RegisterVehicle/Reply";
    pub const LOCATE_VEHICLE_REQ: &str = "rpc/VehicleService/LocateVehicle/Request";
    pub const LOCATE_VEHICLE_REP: &str = "rpc/VehicleService/LocateVehicle/Reply";

    // RouteService
    pub const COMPUTE_ROUTE_REQ: &str = "rpc/RouteService/ComputeRoute/Request";
    pub const COMPUTE_ROUTE_REP: &str = "rpc/RouteService/ComputeRoute/Reply";

    // MaintenanceService
    pub const CREATE_TICKET_REQ: &str = "rpc/MaintenanceService/CreateTicket/Request";
    pub const CREATE_TICKET_REP: &str = "rpc/MaintenanceService/CreateTicket/Reply";
    pub const GET_TICKET_STATUS_REQ: &str = "rpc/MaintenanceService/GetTicketStatus/Request";
    pub const GET_TICKET_STATUS_REP: &str = "rpc/MaintenanceService/GetTicketStatus/Reply";

    // AlertService
    pub const RAISE_ALERT_REQ: &str = "rpc/AlertService/RaiseAlert/Request";
    pub const RAISE_ALERT_REP: &str = "rpc/AlertService/RaiseAlert/Reply";
}

/// Get current timestamp in nanoseconds since UNIX epoch.
pub fn timestamp_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
