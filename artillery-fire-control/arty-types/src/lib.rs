// Artillery Fire Control System Types
//
// Generated from IDL via: hddsgen gen rust idl/artillery.idl -o arty-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

// ---------------------------------------------------------------------------
//  Helper impls for enum display
// ---------------------------------------------------------------------------

impl MissionStatus {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            MissionStatus::PENDING => "Pending",
            MissionStatus::COMPUTING => "Computing",
            MissionStatus::MISSION_FIRING => "Firing",
            MissionStatus::ADJUSTING => "Adjusting",
            MissionStatus::COMPLETE => "Complete",
            MissionStatus::CANCELLED => "Cancelled",
        }
    }
}

impl RoundType {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            RoundType::HE => "HE",
            RoundType::SMOKE => "Smoke",
            RoundType::ILLUM => "Illum",
            RoundType::WP => "WP",
        }
    }
}

impl BatteryState {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            BatteryState::READY => "Ready",
            BatteryState::LOADING => "Loading",
            BatteryState::BATTERY_FIRING => "Firing",
            BatteryState::COOLDOWN => "Cooldown",
            BatteryState::OFFLINE => "Offline",
        }
    }
}

// ---------------------------------------------------------------------------
//  Topic name constants
// ---------------------------------------------------------------------------

pub mod topics {
    pub const BATTERY_STATUS: &str = "arty/battery";
    pub const FIRE_MISSION: &str = "arty/mission";
    pub const BALLISTIC_SOLUTION: &str = "arty/solution";
    pub const ROUND_IN_FLIGHT: &str = "arty/round";
    pub const IMPACT_OBSERVATION: &str = "arty/impact";
    pub const METEO_DATA: &str = "arty/meteo";
}
