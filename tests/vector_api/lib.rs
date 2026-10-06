//! Integration tests for Vector's gRPC API
//!
//! This test suite verifies the gRPC API that powers both `vector top` and `vector tap` commands.
//! Tests cover component discovery, metrics collection, event streaming, and config reloading.

mod common;
mod harness;
mod health;
mod tap;
mod top;
