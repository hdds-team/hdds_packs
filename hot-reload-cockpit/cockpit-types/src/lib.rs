// Hot-Reload Cockpit Types
//
// Generated from IDL via: hddsgen gen rust idl/cockpit.idl -o cockpit-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

/// Topic names used by this demo
pub mod topics {
    pub const SENSOR_FEED: &str = "cockpit/sensor";
    pub const QOS_SNAPSHOT: &str = "cockpit/qos_snapshot";
    pub const METRICS_DELTA: &str = "cockpit/metrics_delta";
}

/// Get current timestamp in nanoseconds
pub fn timestamp_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
