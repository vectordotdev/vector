use std::{path::Path, process::Command};

use anyhow::Result;

use crate::{
    app,
    app::CommandExt,
    testing::{config::RustToolchainConfig, docker::docker_command, test_runner_dockerfile},
    utils::{
        self,
        environment::{Environment, extract_present},
    },
};

pub const ALL_INTEGRATIONS_FEATURE_FLAG: &str = "all-integration-tests";

/// Construct (but do not run) the `docker build` command for a test-runner image.
/// - `image` is the full tag (e.g. `"vector-test-runner-1.86.0:latest"`).
/// - `dockerfile` is the path to the Dockerfile (e.g. `tests/e2e/Dockerfile`).
/// - `features` controls the `FEATURES` build-arg (pass `None` for an empty list).
/// - `build` controls whether to build the Vector binary in the image.
pub fn prepare_build_command(
    image: &str,
    dockerfile: &Path,
    features: Option<&[String]>,
    config_environment_variables: &Environment,
    build: bool,
) -> Command {
    // Start with `docker build`
    let mut command = docker_command(["build"]);

    // Ensure we run from the repo root (so `.` context is correct)
    command.current_dir(app::path());

    // If we're attached to a TTY, show fancy progress
    if *utils::IS_A_TTY {
        command.args(["--progress", "tty"]);
    }

    // Add all of the flags in one go
    command.args([
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
        "--build-arg",
        &format!("BUILD={}", if build { "true" } else { "false" }),
    ]);

    command.envs(extract_present(config_environment_variables));

    command.args(["."]);
    command
}

/// Build the integration test‐runner image from `tests/e2e/Dockerfile`
pub fn build_integration_image() -> Result<()> {
    let dockerfile = test_runner_dockerfile();
    let image = format!("vector-test-runner-{}", RustToolchainConfig::rust_version());
    let mut cmd = prepare_build_command(
        &image,
        &dockerfile,
        Some(&[ALL_INTEGRATIONS_FEATURE_FLAG.to_string()]),
        &Environment::default(),
        false, // Integration tests don't pre-build Vector tests.
    );
    waiting!("Building {image}");
    cmd.check_run()
}
