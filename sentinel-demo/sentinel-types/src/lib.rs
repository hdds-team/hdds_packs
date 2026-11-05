//! Sentinel Demo - DDS Type Definitions
//!
//! Types for vehicle control and telemetry in the tactical demo.
//! Generated from IDL with DDS (CDR) support.

// Generated types from IDL
pub mod generated;

// Re-export all types from generated module
pub use generated::Sentinel::*;

// =====================================================
// TOPIC NAMES
// =====================================================

pub mod topics {
    pub const VEHICLE_POSITION: &str = "Sentinel/VehiclePosition";
    pub const VEHICLE_STATUS: &str = "Sentinel/VehicleStatus";
    pub const FORMATION_COMMAND: &str = "Sentinel/FormationCommand";
    pub const FIRE_COMMAND: &str = "Sentinel/FireCommand";
    pub const COMMAND_ACK: &str = "Sentinel/CommandAck";
    pub const MISSILE_TELEMETRY: &str = "Sentinel/MissileTelemetry";
    pub const IMPACT_EVENT: &str = "Sentinel/ImpactEvent";
}

// NOTE: SentinelMessage JSON wrapper removed -- the regenerated IDL types
// no longer derive serde traits.  Re-add once hdds_gen emits Serialize /
// Deserialize or a "serde" feature flag is wired up.
