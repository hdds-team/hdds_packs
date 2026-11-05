// Mission Recorder Types
//
// Generated from IDL via: hddsgen gen rust idl/mission.idl -o recorder-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

// =============================================================================
// TOPIC NAMES
// =============================================================================

pub mod topics {
    pub const MISSION_EVENT: &str = "mission/event";
    pub const MISSION_STATUS: &str = "mission/status";
}

// =============================================================================
// UTILITY
// =============================================================================

/// Get current wall-clock time as nanoseconds since Unix epoch.
pub fn now_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mission_event_builder() {
        let evt = MissionEvent::builder()
            .event_id(42)
            .event_type("position")
            .source("Alpha")
            .latitude(0.0)
            .longitude(0.0)
            .altitude_m(0.0)
            .heading_deg(0.0)
            .speed_knots(0.0)
            .payload("{}")
            .timestamp_ns(0)
            .build()
            .unwrap();
        assert_eq!(evt.event_id, 42);
        assert_eq!(evt.source, "Alpha");
    }

    #[test]
    fn test_mission_status_builder() {
        let status = MissionStatus::builder()
            .mission_id("TEST-001")
            .phase("planning")
            .elapsed_seconds(0.0)
            .events_recorded(0)
            .participants_active(0)
            .timestamp_ns(0)
            .build()
            .unwrap();
        assert_eq!(status.events_recorded, 0);
        assert_eq!(status.mission_id, "TEST-001");
    }
}
