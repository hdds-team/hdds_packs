// UDS Fleet Diagnostic Types
//
// Generated from IDL via: hddsgen gen rust idl/huds_diag.idl -o diag-types/src/generated.rs
// DO NOT hand-write DDS types -- use hdds_gen.

#![allow(clippy::modulo_one)]

include!("generated.rs");

impl DiagSession {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            DiagSession::DEFAULT => "Default",
            DiagSession::PROGRAMMING => "Programming",
            DiagSession::EXTENDED => "Extended",
        }
    }
}

impl AlertSeverity {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            AlertSeverity::OK => "OK",
            AlertSeverity::WARNING => "Warning",
            AlertSeverity::ALERT_ERROR => "Error",
            AlertSeverity::CRITICAL => "Critical",
        }
    }
}

impl EcuState {
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            EcuState::OFFLINE => "Offline",
            EcuState::IDLE => "Idle",
            EcuState::DIAGNOSING => "Diagnosing",
            EcuState::FLASHING => "Flashing",
            EcuState::ECU_ERROR => "Error",
        }
    }
}
