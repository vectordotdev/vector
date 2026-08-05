//! Common code shared across Antithesis scenarios. Each scenario crate (e.g.
//! `scenarios/vector_to_vector_e2e_disk`) owns its own test-command bins. When two
//! scenarios need the same HTTP or oracle helpers, factor them into modules here.

mod payload;

pub use payload::{decode_payload_field, payload_field, payload_for};
