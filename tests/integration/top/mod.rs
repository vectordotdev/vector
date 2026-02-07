//! Integration tests for `vector top` command
//!
//! These tests verify that the GraphQL API (which `vector top` depends on)
//! correctly exposes component metrics for pipeline monitoring.

mod util;
pub(crate) use util::*;

// Test submodules
mod metrics;
mod reload;
