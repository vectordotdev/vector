//! Integration tests for Vector's GraphQL API
//!
//! This test suite verifies the GraphQL API that powers both `vector top` and `vector tap` commands.
//! Tests cover component discovery, metrics collection, event streaming, and config reloading.

mod common;
mod harness;
mod tap;
mod top;
