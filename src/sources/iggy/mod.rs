//! `Iggy` source.
//!
//! Consumes messages from a topic on the [Iggy](https://iggy.apache.org)
//! message streaming platform and emits them as Vector events.

mod config;
#[cfg(all(test, feature = "iggy-integration-tests"))]
mod integration_tests;
mod source;

pub use config::IggySourceConfig;
