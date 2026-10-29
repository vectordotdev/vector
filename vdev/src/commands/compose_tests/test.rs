use std::iter::once;

use anyhow::{Context, Result, bail};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
    runner::{coverage_filename, local_coverage_output_dir},
};

use super::active_projects::find_active_environment_for_integration;

pub fn exec(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: Option<&String>,
    retries: u8,
    args: &[String],
    coverage: bool,
) -> Result<()> {
    let (_test_dir, config) = ComposeTestConfig::load(local_config.directory, integration)?;
    let envs = config.environments();

    let active =
        find_active_environment_for_integration(local_config.directory, integration, &config)?;
    debug!("Active environment: {active:#?}");

    let environments: Box<dyn Iterator<Item = &String>> = match (environment, &active) {
        (Some(environment), Some(active)) if environment != active => {
            bail!("Requested environment {environment:?} does not match active one {active:?}")
        }
        (Some(environment), _) => Box::new(once(environment)),
        (None, Some(active)) => Box::new(once(active)),
        (None, None) => Box::new(envs.keys()),
    };

    if coverage {
        clear_coverage()?;
    }

    let mut ran_environments = Vec::new();
    for environment in environments {
        run_environment(
            local_config,
            integration,
            environment,
            retries,
            args,
            coverage,
        )?;
        if coverage {
            ran_environments.push(environment.clone());
        }
    }

    if coverage {
        validate_coverage(&ran_environments)?;
    }

    Ok(())
}

pub(crate) fn run_environment(
    local_config: ComposeTestLocalConfig,
    integration: &str,
    environment: &str,
    retries: u8,
    args: &[String],
    coverage: bool,
) -> Result<()> {
    ComposeTest::generate(local_config, integration, environment, retries, coverage)?
        .test(args.to_owned())
}

/// Remove only generated coverage reports so retries cannot reuse stale data.
pub(crate) fn clear_coverage() -> Result<()> {
    let directory = local_coverage_output_dir();
    if directory.exists() {
        for entry in std::fs::read_dir(directory)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "lcov.info"
                || name == "manifest.json"
                || (name.starts_with("lcov-") && name.ends_with(".info"))
            {
                std::fs::remove_file(entry.path())?;
            }
        }
    }
    Ok(())
}

/// Keep each environment's original report. Missing output is a test failure,
/// not a reason to publish an incomplete service report.
pub(crate) fn validate_coverage(environments: &[String]) -> Result<()> {
    for environment in environments {
        let path = local_coverage_output_dir().join(coverage_filename(Some(environment)));
        let contents = std::fs::read_to_string(&path)
            .with_context(|| format!("Missing coverage report {}", path.display()))?;
        anyhow::ensure!(
            !contents.trim().is_empty(),
            "Empty coverage report {}",
            path.display()
        );
    }
    Ok(())
}
