// Satellite Ground Station Network Types
//
// Generated from IDL via: hddsgen gen rust idl/satellite.idl -o sat-types/src/generated.rs
// DO NOT hand-write DDS types -- use hddsgen.

#![allow(clippy::modulo_one)]
#![allow(non_camel_case_types)]

include!("generated.rs");

impl SatStatus {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            SatStatus::ACTIVE => "Active",
            SatStatus::STANDBY => "Standby",
            SatStatus::ECLIPSE => "Eclipse",
            SatStatus::SAFEMODE => "SafeMode",
            SatStatus::LOST => "Lost",
        }
    }
}

impl StationMode {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            StationMode::IDLE => "Idle",
            StationMode::TRACKING => "Tracking",
            StationMode::DOWNLINK => "Downlink",
            StationMode::UPLINK => "Uplink",
            StationMode::HANDOFF => "Handoff",
            StationMode::MAINTENANCE => "Maintenance",
        }
    }
}

impl PassPhase {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            PassPhase::UPCOMING => "Upcoming",
            PassPhase::AOS => "AOS",
            PassPhase::TRACKING_PASS => "Tracking",
            PassPhase::LOS => "LOS",
            PassPhase::COMPLETED => "Completed",
        }
    }
}

pub mod topics {
    pub const SAT_POSITION: &str = "sat/position";
    pub const GROUND_STATION: &str = "sat/station";
    pub const PASS_PREDICTION: &str = "sat/pass";
    pub const TELEMETRY_FRAME: &str = "sat/telemetry";
    pub const ORBITAL_ALERT: &str = "sat/alert";
}
