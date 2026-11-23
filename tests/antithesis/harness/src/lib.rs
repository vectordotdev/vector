//! Shared protocol, payload, and HTTP helpers for Vector's Antithesis scenarios.
//! Scenario binaries contain the property logic; reusable transport mechanics
//! live here so every scenario speaks the same protocol to its oracle and SUT.

mod client;
mod payload;
mod protocol;

pub use client::{all_endpoints_healthy, endpoint_healthy, OracleClient, VectorClient};
pub use payload::{decode_payload_field, payload_field, payload_for};
pub use protocol::{Event, OracleReport};
