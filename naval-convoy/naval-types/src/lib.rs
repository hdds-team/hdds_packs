//! HDDS Naval Convoy Demo - Type Definitions
//!
//! Rust types corresponding to naval.idl for the convoy simulation.
//! DDS-compatible types using `#[derive(DDS)]` for CDR2 encoding.

use hdds::DDS;
use serde::{Deserialize, Serialize};

// =============================================================================
// TOPIC NAMES
// =============================================================================

pub mod topics {
    pub const SHIP: &str = "naval/ship";
    pub const CONVOY_STATUS: &str = "naval/convoy_status";
    pub const SUBMARINE: &str = "naval/submarine";
    pub const SONAR_PING: &str = "naval/sonar_ping";
    pub const SONAR_CONTACT: &str = "naval/sonar_contact";
    pub const TORPEDO: &str = "naval/torpedo";
    pub const DEPTH_CHARGE: &str = "naval/depth_charge";
    pub const EXPLOSION: &str = "naval/explosion";
    pub const COMBAT_EVENT: &str = "naval/combat_event";
    pub const SEA_STATE: &str = "naval/sea_state";
}

// =============================================================================
// ENUMS (as u8 constants for DDS compatibility)
// =============================================================================

/// Ship classification
pub mod ship_class {
    pub const CARRIER: u8 = 0;
    pub const DESTROYER: u8 = 1;
    pub const FRIGATE: u8 = 2;
    pub const CRUISER: u8 = 3;
    pub const CARGO: u8 = 4;
    pub const TANKER: u8 = 5;
    pub const PATROL: u8 = 6;

    pub fn default_speed_kts(class: u8) -> f32 {
        match class {
            CARRIER => 30.0,
            DESTROYER => 33.0,
            FRIGATE => 28.0,
            CRUISER => 32.0,
            CARGO => 15.0,
            TANKER => 14.0,
            PATROL => 25.0,
            _ => 10.0,
        }
    }

    pub fn is_military(class: u8) -> bool {
        !matches!(class, CARGO | TANKER)
    }

    pub fn to_string(class: u8) -> &'static str {
        match class {
            CARRIER => "Carrier",
            DESTROYER => "Destroyer",
            FRIGATE => "Frigate",
            CRUISER => "Cruiser",
            CARGO => "Cargo",
            TANKER => "Tanker",
            PATROL => "Patrol",
            _ => "Unknown",
        }
    }
}

/// Allegiance/faction
pub mod allegiance {
    pub const FRIENDLY: u8 = 0;
    pub const HOSTILE: u8 = 1;
    pub const NEUTRAL: u8 = 2;

    pub fn to_string(a: u8) -> &'static str {
        match a {
            FRIENDLY => "Friendly",
            HOSTILE => "Hostile",
            NEUTRAL => "Neutral",
            _ => "Unknown",
        }
    }
}

/// Submarine operational state
pub mod sub_state {
    pub const SURFACED: u8 = 0;
    pub const PERISCOPE: u8 = 1;
    pub const SHALLOW: u8 = 2;
    pub const DEEP: u8 = 3;
    pub const EMERGENCY: u8 = 4;

    /// Typical depth for state (meters, negative = underwater)
    pub fn typical_depth_m(state: u8) -> f32 {
        match state {
            SURFACED => 0.0,
            PERISCOPE => -15.0,
            SHALLOW => -75.0,
            DEEP => -200.0,
            EMERGENCY => -250.0,
            _ => 0.0,
        }
    }

    /// Noise multiplier (affects detection)
    pub fn noise_factor(state: u8) -> f32 {
        match state {
            SURFACED => 1.0,
            PERISCOPE => 0.7,
            SHALLOW => 0.4,
            DEEP => 0.2,
            EMERGENCY => 0.8,
            _ => 1.0,
        }
    }

    pub fn to_string(state: u8) -> &'static str {
        match state {
            SURFACED => "Surfaced",
            PERISCOPE => "Periscope",
            SHALLOW => "Shallow",
            DEEP => "Deep",
            EMERGENCY => "Emergency",
            _ => "Unknown",
        }
    }
}

/// Torpedo engagement phase
pub mod torpedo_phase {
    pub const LAUNCHED: u8 = 0;
    pub const SEARCHING: u8 = 1;
    pub const ACQUIRED: u8 = 2;
    pub const TERMINAL: u8 = 3;
    pub const DETONATED: u8 = 4;
    pub const MISSED: u8 = 5;

    pub fn is_active(phase: u8) -> bool {
        !matches!(phase, DETONATED | MISSED)
    }

    pub fn to_string(phase: u8) -> &'static str {
        match phase {
            LAUNCHED => "Launched",
            SEARCHING => "Searching",
            ACQUIRED => "Acquired",
            TERMINAL => "Terminal",
            DETONATED => "Detonated",
            MISSED => "Missed",
            _ => "Unknown",
        }
    }
}

/// Sonar system type
pub mod sonar_type {
    pub const ACTIVE_HULL: u8 = 0;
    pub const ACTIVE_TOWED: u8 = 1;
    pub const PASSIVE: u8 = 2;
    pub const DIPPING: u8 = 3;

    pub fn to_string(t: u8) -> &'static str {
        match t {
            ACTIVE_HULL => "ActiveHull",
            ACTIVE_TOWED => "ActiveTowed",
            PASSIVE => "Passive",
            DIPPING => "Dipping",
            _ => "Unknown",
        }
    }
}

/// Contact classification confidence
pub mod contact_confidence {
    pub const UNKNOWN: u8 = 0;
    pub const POSSIBLE: u8 = 1;
    pub const PROBABLE: u8 = 2;
    pub const CONFIRMED: u8 = 3;

    pub fn to_string(c: u8) -> &'static str {
        match c {
            UNKNOWN => "Unknown",
            POSSIBLE => "Possible",
            PROBABLE => "Probable",
            CONFIRMED => "Confirmed",
            _ => "Unknown",
        }
    }
}

// =============================================================================
// SURFACE VESSELS
// =============================================================================

/// Surface ship - convoy members and escorts
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct Ship {
    pub ship_id: u32,
    pub name: String,
    pub ship_class: u8,         // ship_class::*
    pub allegiance: u8,         // allegiance::*

    // Position (nautical miles from origin)
    pub x_nm: f32,
    pub y_nm: f32,
    pub heading_deg: f32,
    pub speed_kts: f32,

    // Status
    pub hull_integrity: f32,
    pub active_sonar: u8,       // 0 = off, 1 = on (bool not supported by DDS derive)
    pub torpedoes_remaining: u8,
    pub depth_charges_remaining: u8,

    pub timestamp_us: u64,
}

impl Ship {
    pub fn new(ship_id: u32, name: &str, class: u8, alleg: u8) -> Self {
        Self {
            ship_id,
            name: name.to_string(),
            ship_class: class,
            allegiance: alleg,
            x_nm: 0.0,
            y_nm: 0.0,
            heading_deg: 0.0,
            speed_kts: ship_class::default_speed_kts(class),
            hull_integrity: 1.0,
            active_sonar: if ship_class::is_military(class) { 1 } else { 0 },
            torpedoes_remaining: if ship_class::is_military(class) { 8 } else { 0 },
            depth_charges_remaining: if matches!(class, ship_class::DESTROYER | ship_class::FRIGATE) { 24 } else { 0 },
            timestamp_us: 0,
        }
    }

    pub fn is_sonar_active(&self) -> bool {
        self.active_sonar != 0
    }
}

/// Convoy formation status
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct ConvoyStatus {
    pub convoy_id: u32,
    pub convoy_name: String,
    pub center_x_nm: f32,
    pub center_y_nm: f32,
    pub base_heading_deg: f32,
    pub base_speed_kts: f32,
    pub ship_count: u8,
    pub escort_count: u8,
    pub status_message: String,
    pub timestamp_us: u64,
}

// =============================================================================
// SUBMARINES
// =============================================================================

/// Submarine vessel
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct Submarine {
    pub sub_id: u32,
    pub name: String,
    pub allegiance: u8,         // allegiance::*

    // Position
    pub x_nm: f32,
    pub y_nm: f32,
    pub depth_m: f32,
    pub heading_deg: f32,
    pub speed_kts: f32,

    // State
    pub state: u8,              // sub_state::*
    pub battery_percent: f32,
    pub noise_level: f32,
    pub torpedoes_remaining: u8,

    pub distance_to_convoy_nm: f32,

    pub timestamp_us: u64,
}

impl Submarine {
    pub fn new(sub_id: u32, name: &str, alleg: u8) -> Self {
        Self {
            sub_id,
            name: name.to_string(),
            allegiance: alleg,
            x_nm: 0.0,
            y_nm: 0.0,
            depth_m: -75.0,
            heading_deg: 0.0,
            speed_kts: 8.0,
            state: sub_state::SHALLOW,
            battery_percent: 100.0,
            noise_level: 0.3,
            torpedoes_remaining: 6,
            distance_to_convoy_nm: 0.0,
            timestamp_us: 0,
        }
    }

    /// Calculate current noise level based on state and speed
    pub fn calculate_noise(&self) -> f32 {
        let base_noise = sub_state::noise_factor(self.state);
        let speed_factor = (self.speed_kts / 20.0).min(1.0);
        (base_noise + speed_factor * 0.5).min(1.0)
    }
}

// =============================================================================
// SONAR SYSTEM
// =============================================================================

/// Active sonar ping emission
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct SonarPing {
    pub ping_id: u32,
    pub source_id: u32,
    pub sonar_type: u8,         // sonar_type::*
    pub x_nm: f32,
    pub y_nm: f32,
    pub radius_nm: f32,
    pub max_range_nm: f32,
    pub bearing_deg: f32,
    pub beam_width_deg: f32,
    pub timestamp_us: u64,
}

/// Sonar contact detection
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct SonarContact {
    pub contact_id: u32,
    pub detector_id: u32,

    pub bearing_deg: f32,
    pub range_nm: f32,
    pub range_uncertainty_nm: f32,
    pub depth_estimate_m: f32,

    pub confidence: u8,         // contact_confidence::*
    pub classification: String,
    pub signal_strength: f32,

    pub estimated_speed_kts: f32,
    pub estimated_heading_deg: f32,
    pub first_detected_us: u64,
    pub last_update_us: u64,
}

// =============================================================================
// WEAPONS
// =============================================================================

/// Torpedo weapon
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct Torpedo {
    pub torpedo_id: u32,
    pub launcher_id: u32,
    pub target_id: u32,
    pub allegiance: u8,         // allegiance::*

    pub x_nm: f32,
    pub y_nm: f32,
    pub depth_m: f32,
    pub heading_deg: f32,
    pub speed_kts: f32,

    pub phase: u8,              // torpedo_phase::*
    pub fuel_remaining: f32,
    pub search_radius_nm: f32,

    pub timestamp_us: u64,
}

impl Torpedo {
    pub fn new(torpedo_id: u32, launcher_id: u32, alleg: u8, x: f32, y: f32, heading: f32) -> Self {
        Self {
            torpedo_id,
            launcher_id,
            target_id: 0,
            allegiance: alleg,
            x_nm: x,
            y_nm: y,
            depth_m: -10.0,
            heading_deg: heading,
            speed_kts: 45.0,
            phase: torpedo_phase::LAUNCHED,
            fuel_remaining: 1.0,
            search_radius_nm: 0.5,
            timestamp_us: 0,
        }
    }

    pub fn is_active(&self) -> bool {
        torpedo_phase::is_active(self.phase)
    }
}

/// Depth charge anti-submarine weapon
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct DepthCharge {
    pub charge_id: u32,
    pub launcher_id: u32,

    pub x_nm: f32,
    pub y_nm: f32,
    pub depth_m: f32,
    pub set_depth_m: f32,
    pub sink_rate_m_s: f32,

    pub detonated: u8,          // 0 = false, 1 = true
    pub blast_radius_m: f32,

    pub timestamp_us: u64,
}

impl DepthCharge {
    pub fn new(charge_id: u32, launcher_id: u32, x: f32, y: f32, set_depth: f32) -> Self {
        Self {
            charge_id,
            launcher_id,
            x_nm: x,
            y_nm: y,
            depth_m: 0.0,
            set_depth_m: set_depth,
            sink_rate_m_s: 4.0,
            detonated: 0,
            blast_radius_m: 50.0,
            timestamp_us: 0,
        }
    }

    pub fn is_detonated(&self) -> bool {
        self.detonated != 0
    }
}

// =============================================================================
// EVENTS
// =============================================================================

/// Explosion event
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct Explosion {
    pub explosion_id: u32,
    pub x_nm: f32,
    pub y_nm: f32,
    pub depth_m: f32,
    pub magnitude: f32,
    pub explosion_type: String,
    pub source_id: u32,
    pub victim_id: u32,
    pub timestamp_us: u64,
}

/// Combat event for logging/display
#[derive(Debug, Clone, Default, Serialize, Deserialize, DDS)]
pub struct CombatEvent {
    pub event_id: u32,
    pub event_type: String,
    pub description: String,
    pub actor_id: u32,
    pub target_id: u32,
    pub priority: u8,
    pub timestamp_us: u64,
}

// =============================================================================
// ENVIRONMENT
// =============================================================================

/// Sea state conditions
#[derive(Debug, Clone, Serialize, Deserialize, DDS)]
pub struct SeaState {
    pub zone_id: u32,
    pub wave_height_m: f32,
    pub wave_period_s: f32,
    pub wave_direction_deg: f32,
    pub wind_speed_kts: f32,
    pub wind_direction_deg: f32,
    pub visibility_nm: f32,
    pub thermal_layer_depth_m: f32,
    pub timestamp_us: u64,
}

impl Default for SeaState {
    fn default() -> Self {
        Self {
            zone_id: 0,
            wave_height_m: 1.5,
            wave_period_s: 6.0,
            wave_direction_deg: 45.0,
            wind_speed_kts: 15.0,
            wind_direction_deg: 45.0,
            visibility_nm: 10.0,
            thermal_layer_depth_m: 100.0,
            timestamp_us: 0,
        }
    }
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/// Get current timestamp in microseconds
pub fn timestamp_us() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// Calculate distance between two points in nautical miles
pub fn distance_nm(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    ((x2 - x1).powi(2) + (y2 - y1).powi(2)).sqrt()
}

/// Calculate bearing from point 1 to point 2 (degrees, 0 = North)
pub fn bearing_deg(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let angle = dy.atan2(dx).to_degrees();
    (90.0 - angle + 360.0) % 360.0
}

/// Normalize angle to 0-360
pub fn normalize_angle(angle: f32) -> f32 {
    ((angle % 360.0) + 360.0) % 360.0
}

/// Convert knots to nautical miles per hour (they're the same!)
pub fn kts_to_nm_per_hour(kts: f32) -> f32 {
    kts
}

/// Convert knots to nautical miles per second
pub fn kts_to_nm_per_sec(kts: f32) -> f32 {
    kts / 3600.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distance() {
        assert!((distance_nm(0.0, 0.0, 3.0, 4.0) - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_bearing() {
        // North
        assert!((bearing_deg(0.0, 0.0, 0.0, 1.0) - 0.0).abs() < 0.001);
        // East
        assert!((bearing_deg(0.0, 0.0, 1.0, 0.0) - 90.0).abs() < 0.001);
        // South
        assert!((bearing_deg(0.0, 0.0, 0.0, -1.0) - 180.0).abs() < 0.001);
    }

    #[test]
    fn test_ship_class_helpers() {
        assert!(ship_class::is_military(ship_class::DESTROYER));
        assert!(!ship_class::is_military(ship_class::CARGO));
        assert!((ship_class::default_speed_kts(ship_class::CARRIER) - 30.0).abs() < 0.001);
    }

    #[test]
    fn test_sub_state_helpers() {
        assert!((sub_state::typical_depth_m(sub_state::DEEP) - (-200.0)).abs() < 0.001);
        assert!((sub_state::noise_factor(sub_state::DEEP) - 0.2).abs() < 0.001);
    }

    #[test]
    fn test_torpedo_phase() {
        assert!(torpedo_phase::is_active(torpedo_phase::LAUNCHED));
        assert!(!torpedo_phase::is_active(torpedo_phase::DETONATED));
        assert!(!torpedo_phase::is_active(torpedo_phase::MISSED));
    }
}
