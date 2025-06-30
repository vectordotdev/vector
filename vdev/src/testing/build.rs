use crate::app;
use crate::app::CommandExt;
use crate::testing::config::RustToolchainConfig;
use crate::testing::docker::docker_command;
use crate::util::IS_A_TTY;
use anyhow::Result;
use std::path::PathBuf;
use std::{path::Path, process::Command};

pub const ALL_INTEGRATIONS_FEATURE_FLAG: &str = "all-integration-tests";

/// Construct (but do not run) the `docker build` command for a test-runner image.
/// - `image` is the full tag (e.g. `"vector-test-runner-1.86.0:latest"`).
/// - `dockerfile` is the path to the Dockerfile (e.g. `scripts/integration/Dockerfile`).
/// - `features` controls the `FEATURES` build-arg (pass `None` for an empty list).
pub fn prepare_build_command(
    image: &str,
    dockerfile: &Path,
    features: Option<&[String]>,
) -> Command {
    // Start with `docker build`
    let mut cmd = docker_command(["build"]);

    // Ensure we run from the repo root (so `.` context is correct)
    cmd.current_dir(app::path());

    // If we're attached to a TTY, show fancy progress
    if *IS_A_TTY {
        cmd.args(["--progress", "tty"]);
    }

    // Add all of the flags in one go
    cmd.args([
        "--pull",
        "--tag",
        image,
        "--file",
        dockerfile.to_str().unwrap(),
        "--label",
        "vector-test-runner=true",
        "--build-arg",
        &format!("RUST_VERSION={}", RustToolchainConfig::rust_version()),
        "--build-arg",
        &format!("FEATURES={}", features.unwrap_or(&[]).join(",")),
        ".",
    ]);

    cmd
}

#[allow(dead_code)]
/// Build the integration test‐runner image from `scripts/integration/Dockerfile`
pub fn build_integration_image() -> Result<()> {
    let dockerfile: PathBuf = [app::path(), "scripts", "integration", "Dockerfile"]
        .iter()
        .collect();
    let image = format!("vector-test-runner-{}", RustToolchainConfig::rust_version());
    let mut cmd = prepare_build_command(
        &image,
        &dockerfile,
        Some(&[ALL_INTEGRATIONS_FEATURE_FLAG.to_string()]),
    );
    waiting!("Building {image}");
    cmd.check_run()
}
