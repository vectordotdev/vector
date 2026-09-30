use anyhow::Result;
use std::process::Command;

use crate::{
    app::CommandExt as _,
    testing::{
        config::ComposeTestConfig,
        docker::CONTAINER_TOOL,
        integration::{ComposeTestLocalConfig, collect_compose_logs},
    },
};

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
    retries: u8,
    coverage: bool,
    always_show_logs: bool,
) -> Result<()> {
    let resolved_envs: Vec<String> = if environments.is_empty() {
        let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, test_name)?;
        config.environments().keys().cloned().collect()
    } else {
        environments.to_vec()
    };

    if resolved_envs.is_empty() {
        anyhow::bail!("No environments found for test '{test_name}'");
    }

    if coverage {
        super::test::clear_coverage();
    }

    let mut covered_environments = Vec::new();
    for environment in &resolved_envs {
        info!("Running test '{test_name}' in environment '{environment}'");

        cleanup_test_output(test_name)?;

        let start_result = super::start::exec(local_config, test_name, Some(environment));

        if let Err(ref e) = start_result {
            error!("Failed to start environment: {e}");
            print_compose_logs(local_config, test_name, environment);
        }

        let test_result = if start_result.is_ok() {
            let result = super::test::run_environment(
                local_config,
                test_name,
                environment,
                retries,
                &[],
                coverage,
            );

            if let Err(ref e) = result {
                error!("Tests failed: {e}");
                print_compose_logs(local_config, test_name, environment);
            } else if always_show_logs {
                print_compose_logs(local_config, test_name, environment);
            }

            upload_test_results();

            if result.is_ok() && coverage {
                covered_environments.push(environment.clone());
            }

            result
        } else {
            warn!("Skipping test phase because 'start' failed");
            start_result
        };

        // Always stop the environment (best effort cleanup)
        if let Err(e) = super::stop::exec(local_config, test_name) {
            warn!("Failed to stop environment (cleanup): {e}");
        }

        // Exit early on first failure
        test_result?;
    }

    if coverage {
        super::test::merge_coverage(&covered_environments)?;
    }

    Ok(())
}

/// Print docker compose logs for the given test environment.
fn print_compose_logs(local_config: ComposeTestLocalConfig, test_name: &str, environment: &str) {
    info!("Collecting compose logs for test '{test_name}' in environment '{environment}'...");

    match collect_compose_logs(local_config, test_name, environment) {
        Ok(Some(status)) if !status.success() => {
            warn!("Failed to collect logs: docker compose exited with {status}");
        }
        Ok(Some(_) | None) => {}
        Err(e) => warn!("Failed to collect logs: {e}"),
    }
}

/// Clean up previous test output from the docker volume
fn cleanup_test_output(test_name: &str) -> Result<()> {
    debug!("Cleaning up previous test output for '{test_name}'");

    let status = Command::new(CONTAINER_TOOL.clone())
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
        .map_err(|e| anyhow::anyhow!("Failed to run cleanup command: {e}"))?;

    if !status.success() {
        warn!("Failed to clean up previous test output (this may be okay if it didn't exist)");
    }

    Ok(())
}

/// Upload test results to Datadog (in CI only, no-op locally)
///
/// The script itself checks for CI environment and handles the logic.
fn upload_test_results() {
    let result = Command::script("upload-test-results.sh").status();

    match result {
        Ok(status) if !status.success() => {
            warn!("Upload script exited with non-zero status");
        }
        Err(e) => {
            warn!("Failed to execute upload script: {e}");
        }
        _ => {}
    }
}
