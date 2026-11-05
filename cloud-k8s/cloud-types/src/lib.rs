// Cloud K8s Types
//
// Generated from IDL via: hddsgen gen rust idl/cloud_native.idl -o cloud-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

/// Topic name constants
pub mod topics {
    pub const WORKLOAD_METRIC: &str = "cloud/workload_metric";
    pub const DISCOVERY_EVENT: &str = "cloud/discovery_event";
    pub const CROSS_SITE_MESSAGE: &str = "cloud/cross_site_message";
    pub const SCALING_DECISION: &str = "cloud/scaling_decision";
}

/// Get current timestamp in nanoseconds
pub fn timestamp_ns() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as i64
}
