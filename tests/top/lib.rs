//! Integration tests for `vector top` command
//!
//! These tests verify that the GraphQL API (which `vector top` depends on)
//! correctly exposes component metrics for pipeline monitoring.

mod harness;
mod metrics;
mod reload;
