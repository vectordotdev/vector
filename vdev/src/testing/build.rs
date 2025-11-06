use std::{path::Path, process::Command};

use anyhow::Result;

use crate::testing::integration::ComposeTestKind;
use crate::testing::test_runner_dockerfile;
use crate::{
    app,
    app::CommandExt,
    testing::{config::RustToolchainConfig, docker::docker_command},
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
/// - `build_tests` controls the `BUILD_TESTS` build-arg (true to pre-build test binaries and Vector binary in the image).
pub fn prepare_build_command(
    image: &str,
    dockerfile: &Path,
    features: Option<&[String]>,
    config_environment_variables: &Environment,
    build_tests: bool,
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
        &format!("BUILD_TESTS={}", if build_tests { "true" } else { "false" }),
    ]);

    command.envs(extract_present(config_environment_variables));

    command.args(["."]);
    command
}

pub const ALL_E2E_FEATURE_FLAG: &str = "all-e2e-tests";

/// Build a test runner image for the given test kind
pub fn build_test_runner_image(
    test_kind: ComposeTestKind,
    feature_flag: &str,
    build_tests: bool,
) -> Result<()> {
    use crate::testing::docker::docker_command;

    // Clean up any existing containers using this image
    let image_name = test_kind.image_name();
    info!("Cleaning up existing test runner containers for {image_name}");

    // Find and remove containers using this image
    if let Ok(output) = docker_command([
        "ps",
        "-a",
        "--filter",
        &format!("ancestor={image_name}"),
        "--format",
        "{{.ID}}",
    ])
    .output()
    {
        if output.status.success() {
            let container_ids = String::from_utf8_lossy(&output.stdout);
            for container_id in container_ids.lines().filter(|s| !s.is_empty()) {
                let _ = docker_command(["rm", "--force", container_id]).output();
            }
        }
    }

    // Clean up the shared target volume
    info!("Cleaning up vector_target volume");
    let _ = docker_command(["volume", "rm", "vector_target"]).output();

    // Remove the old image and prune build cache to ensure fresh build
    info!("Removing old image {image_name}:latest");
    let _ = docker_command(["rmi", &format!("{image_name}:latest")]).output();

    info!("Pruning Docker build cache");
    let _ = docker_command(["builder", "prune", "--force"]).output();

    let dockerfile = test_runner_dockerfile();
    let image = format!("{image_name}:latest");
    let mut cmd = prepare_build_command(
        &image,
        &dockerfile,
        Some(&[feature_flag.to_string()]),
        &Environment::default(),
        build_tests,
    );
    waiting!("Building {image}");
    cmd.check_run()
}
