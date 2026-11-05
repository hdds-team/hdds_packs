//! DDS Types for Air Defense Radar Demo
//!
//! Types for radar sweep, aircraft tracks, SAM sites, and engagements.

use hdds::DDS;
use serde::{Deserialize, Serialize};

/// Track classification status (as u8 constants)
pub mod track_status {
    pub const UNKNOWN: u8 = 0;
    pub const FRIENDLY: u8 = 1;
    pub const HOSTILE: u8 = 2;
    pub const NEUTRAL: u8 = 3;

    pub fn to_string(status: u8) -> &'static str {
        match status {
            FRIENDLY => "Friendly",
            HOSTILE => "Hostile",
            NEUTRAL => "Neutral",
            _ => "Unknown",
        }
    }
}

/// Engagement phase (as u8 constants)
pub mod engagement_phase {
    pub const TRACKING: u8 = 0;
    pub const LAUNCHING: u8 = 1;
    pub const INTERCEPTING: u8 = 2;
    pub const COMPLETE: u8 = 3;
}

/// Radar sweep position - published at ~90 deg/s (15 RPM)
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct RadarSweep {
    pub radar_id: u32,
    pub azimuth_deg: f32,      // 0-360 current sweep angle
    pub range_km: f32,         // Maximum detection range
    pub timestamp_us: u64,
}

/// Aircraft track - detected targets
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct AircraftTrack {
    pub track_id: u32,
    pub x_km: f32,             // East-West position (+ = East)
    pub y_km: f32,             // North-South position (+ = North)
    pub altitude_m: f32,       // Altitude in meters
    pub heading_deg: f32,      // 0-360, 0 = North
    pub speed_kts: f32,        // Speed in knots
    pub status: u8,            // TrackStatus as u8
    pub callsign: String,      // e.g., "UA-1234", "RU-5678"
    pub rcs: f32,              // Radar cross-section (dBsm)
    pub timestamp_us: u64,
}

/// SAM site status
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct SamSite {
    pub site_id: u32,
    pub x_km: f32,             // Site position
    pub y_km: f32,
    pub range_km: f32,         // Engagement range
    pub missiles_ready: u8,    // Available missiles
    pub missiles_total: u8,    // Total capacity
    pub site_name: String,     // e.g., "PATRIOT-1", "S-300"
    pub active: u8,            // Radar on/off (0=off, 1=on)
}

/// Active engagement
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct Engagement {
    pub engagement_id: u32,
    pub site_id: u32,          // Launching SAM site
    pub track_id: u32,         // Target aircraft
    pub phase: u8,             // EngagementPhase as u8
    pub missile_x_km: f32,     // Missile position
    pub missile_y_km: f32,
    pub missile_alt_m: f32,
    pub intercept_x_km: f32,   // Predicted intercept point
    pub intercept_y_km: f32,
    pub time_to_intercept_s: f32,
    pub timestamp_us: u64,
}

/// Engagement result
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct EngagementResult {
    pub engagement_id: u32,
    pub track_id: u32,
    pub site_id: u32,
    pub success: u8,           // 1 = kill, 0 = miss
    pub details: String,       // "Target destroyed", etc.
    pub timestamp_us: u64,
}

/// Radar alert - threat warnings
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct RadarAlert {
    pub alert_id: u32,
    pub track_id: u32,         // Related track (if any)
    pub alert_type: String,    // "NEW_HOSTILE", "ENTERING_RANGE", etc.
    pub message: String,
    pub priority: u8,          // 0=info, 1=warning, 2=critical
    pub timestamp_us: u64,
}

/// Topic names
pub mod topics {
    pub const RADAR_SWEEP: &str = "radar/sweep";
    pub const AIRCRAFT_TRACK: &str = "radar/tracks";
    pub const SAM_SITE: &str = "radar/sam_sites";
    pub const ENGAGEMENT: &str = "radar/engagements";
    pub const ENGAGEMENT_RESULT: &str = "radar/engagement_results";
    pub const RADAR_ALERT: &str = "radar/alerts";
}

/// Get current timestamp in microseconds
pub fn timestamp_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}
