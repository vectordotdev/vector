use std::path::PathBuf;

use crate::app;

pub mod build;
pub mod config;
pub mod docker;
pub mod integration;
pub mod runner;

/// Returns the path to the unified test runner Dockerfile.
/// Both integration and E2E tests use the same Dockerfile at `tests/e2e/Dockerfile`.
pub fn test_runner_dockerfile() -> PathBuf {
    [app::path(), "tests", "e2e", "Dockerfile"].iter().collect()
}
