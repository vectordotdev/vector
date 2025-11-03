use anyhow::{Context, Result};
use std::process::Command;

use crate::testing::{config::ComposeTestConfig, integration::ComposeTestLocalConfig};

/// Run a complete test workflow orchestrating start, test, and stop phases
///
/// This function implements the full test lifecycle used in CI:
/// 1. Clean up previous test output
/// 2. Start the environment
/// 3. Run tests with retries
/// 4. Upload results to Datadog (in CI)
/// 5. Stop the environment (always, as cleanup)
pub fn exec(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
    environments: &[String],
    build_all: bool,
    reuse_image: bool,
    retries: u8,
    show_logs: bool,
) -> Result<()> {
    let environments = if environments.is_empty() {
        // Auto-discover environments
        let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, test_name)?;
        config.environments().keys().cloned().collect()
    } else {
        environments.to_vec()
    };

    if environments.is_empty() {
        anyhow::bail!("No environments found for test '{test_name}'");
    }

    for environment in &environments {
        info!("Running test '{test_name}' in environment '{environment}'");

        // Clean up previous test output
        cleanup_test_output(test_name)?;

        // Start the environment
        let start_result = super::start::exec(
            local_config,
            test_name,
            Some(environment),
            build_all,
            reuse_image,
        );

        if let Err(e) = &start_result {
            error!("Failed to start environment: {e}");
            if show_logs || is_debug_mode() {
                print_compose_logs(test_name);
            }
        }

        let test_result = if start_result.is_ok() {
            // Run tests
            let result = super::test::exec(
                local_config,
                test_name,
                Some(environment),
                build_all,
                reuse_image,
                retries,
                &[],
            );

            if let Err(e) = &result {
                error!("Tests failed: {e}");
                if show_logs || is_debug_mode() {
                    print_compose_logs(test_name);
                }
            }

            // Upload test results (only in CI)
            upload_test_results();

            result
        } else {
            warn!("Skipping test phase because 'start' failed");
            start_result
        };

        // Always stop the environment (best effort cleanup)
        if let Err(e) = super::stop::exec(local_config, test_name, build_all, reuse_image) {
            warn!("Failed to stop environment (cleanup): {e}");
        }

        // Exit early on first failure
        test_result?;
    }

    Ok(())
}

/// Check if we're running in debug mode (`ACTIONS_RUNNER_DEBUG` or `RUST_LOG`)
fn is_debug_mode() -> bool {
    std::env::var("ACTIONS_RUNNER_DEBUG")
        .map(|v| v == "true")
        .unwrap_or(false)
        || std::env::var("RUST_LOG")
            .map(|v| v.contains("debug") || v.contains("trace"))
            .unwrap_or(false)
}

/// Print docker compose logs for debugging
fn print_compose_logs(project_name: &str) {
    info!("Collecting docker compose logs for project '{project_name}'...");

    let result = Command::new("docker")
        .args(["compose", "--project-name", project_name, "logs"])
        .status();

    if let Err(e) = result {
        warn!("Failed to collect logs: {e}");
    }
}

/// Clean up previous test output from the docker volume
fn cleanup_test_output(test_name: &str) -> Result<()> {
    debug!("Cleaning up previous test output for '{test_name}'");

    let status = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &format!("vector_target:/output/{test_name}"),
            "alpine:3.20",
            "sh",
            "-c",
            &format!("rm -rf /output/{test_name}/*"),
        ])
        .status()
        .context("Failed to run docker cleanup command")?;

    if !status.success() {
        warn!("Failed to clean up previous test output (this may be okay if it didn't exist)");
    }

    Ok(())
}

/// Upload test results to Datadog (in CI only, no-op locally)
///
/// The script itself checks for CI environment and handles the logic.
fn upload_test_results() {
    // Get the repo root path
    let script_path =
        std::path::PathBuf::from(crate::app::path()).join("scripts/upload-test-results.sh");

    // Call the upload script (it checks for CI internally)
    let result = Command::new(&script_path).status();

    match result {
        Ok(status) if !status.success() => {
            warn!("Upload script exited with non-zero status");
        }
        Err(e) => {
            warn!("Failed to execute upload script: {e}");
        }
        _ => {} // Success or handled by script
    }
}
