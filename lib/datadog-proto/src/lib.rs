//! Generated Datadog Agent protobuf types.
//!
//! This crate owns the wire types for Datadog traces (`datadog.trace` and
//! `datadog.trace.idx`) and metrics (`datadog.agentpayload` and
//! `datadoghq.api.metrics.v3`).

#![allow(clippy::derive_partial_eq_without_eq)]

/// Generated types for `datadog.agentpayload` (metrics series and sketches).
#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod agentpayload {
    include!(concat!(env!("OUT_DIR"), "/datadog.agentpayload.rs"));
}

/// Generated types for `datadoghq.api.metrics.v3` (metrics intake v3).
#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod metrics_v3 {
    include!(concat!(env!("OUT_DIR"), "/datadoghq.api.metrics.v3.rs"));
}

/// Generated types for `datadog.trace` (AgentPayload / TracerPayload / TraceChunk / Span).
#[allow(warnings, clippy::all, clippy::pedantic, clippy::nursery)]
pub mod trace {
    /// Generated types for `datadog.trace.idx`.
    pub mod idx {
        include!(concat!(env!("OUT_DIR"), "/datadog.trace.idx.rs"));
    }

    include!(concat!(env!("OUT_DIR"), "/datadog.trace.rs"));
}

include!(concat!(env!("OUT_DIR"), "/datadog-proto.rs"));
