//! The `iggy` sink: publish OTLP telemetry to an Obstack cluster through
//! Apache Iggy, replacing the reference OpenTelemetry Collector + Obstack's
//! `obstack-otel-iggy` adapter with a single Vector component.
//!
//! Pair it with an `opentelemetry` source configured with
//! `use_otlp_decoding: true` so the OTLP structure is preserved end to end.

mod config;
mod otlp;
mod proto;
mod publisher;
mod sink;

pub use config::IggySinkConfig;

#[cfg(test)]
mod tests;
