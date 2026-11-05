// Fort Knox Security Types
//
// Generated from IDL via: hddsgen gen rust idl/fort_knox.idl -o knox-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]
#![allow(non_camel_case_types)]

include!("generated.rs");

// ---------------------------------------------------------------------------
// Enum helper methods
// ---------------------------------------------------------------------------

/// Number of security zones.
pub const ZONE_COUNT: usize = 5;

impl Zone {
    /// Human-readable zone name.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Zone::PUBLIC_LOBBY => "Public Lobby",
            Zone::TELLER_AREA => "Teller Area",
            Zone::VAULT_CORRIDOR => "Vault Corridor",
            Zone::VAULT_ROOM => "Vault Room",
            Zone::SERVER_ROOM => "Server Room",
        }
    }
}

impl Severity {
    /// Human-readable severity label.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Severity::INFO => "INFO",
            Severity::WARNING => "WARNING",
            Severity::CRITICAL => "CRITICAL",
            Severity::BREACH => "BREACH",
        }
    }
}

impl SensorType {
    /// Human-readable sensor type label.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            SensorType::CAMERA => "Camera",
            SensorType::MOTION => "Motion",
            SensorType::DOOR => "Door",
            SensorType::VIBRATION => "Vibration",
            SensorType::THERMAL => "Thermal",
            SensorType::WEIGHT => "Weight",
        }
    }
}

// ---------------------------------------------------------------------------
// Topic names
// ---------------------------------------------------------------------------

pub mod topics {
    pub const SENSOR_READING: &str = "knox/sensors";
    pub const SECURITY_ALERT: &str = "knox/alerts";
    pub const ACCESS_EVENT: &str = "knox/access";
    pub const VAULT_STATUS: &str = "knox/vault";
    pub const AUDIT_LOG: &str = "knox/audit";
    pub const PERMISSION_CHANGE: &str = "knox/permissions";
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get current time as nanoseconds since UNIX epoch.
pub fn timestamp_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
