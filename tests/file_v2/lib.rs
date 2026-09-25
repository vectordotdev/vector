//! Real-binary file_v2 tests. Run with `make test-file_v2`.
//!
//! Scenarios use real files and source-filtered internal metrics. Readiness comes
//! from observed output; timeouts bound failures without startup sleeps.

#![cfg(unix)]

mod common;
mod deletion;
mod discovery;
mod fairness;
mod gzip;
mod harness;
mod restart;
mod rotation;
mod shutdown;
