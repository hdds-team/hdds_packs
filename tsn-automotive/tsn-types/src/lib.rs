// TSN Automotive Types
//
// Generated from IDL via: hddsgen gen rust idl/tsn_vehicle.idl -o src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

/// Human-readable tier name.
impl PriorityTier {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            PriorityTier::CRITICAL => "CRITICAL",
            PriorityTier::HIGH => "HIGH",
            PriorityTier::MEDIUM => "MEDIUM",
            PriorityTier::LOW => "LOW",
        }
    }
}
