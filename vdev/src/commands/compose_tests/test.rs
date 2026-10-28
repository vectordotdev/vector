use std::iter::once;

use anyhow::{Context, Result, bail};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
    runner::{coverage_filename, local_coverage_output_dir},
};

use super::active_projects::find_active_environment_for_integration;

/// RAII guard that triggers test environment cleanup.
///
/// Cleanup is skipped if disabled via `AUTODESPAWN` or `AUTOSPAWN` environment
/// variables, or if the environment was already running prior to the test execution
/// (preserving caller-owned environments and fast repeated runs).
struct DespawnGuard<'a> {
    local_config: ComposeTestLocalConfig,
    test_name: &'a str,
    should_despawn: bool,
}

impl<'a> DespawnGuard<'a> {
    pub fn new(
        local_config: ComposeTestLocalConfig,
        test_name: &'a str,
        was_running: bool,
    ) -> Self {
        let auto_despawn = std::env::var("AUTODESPAWN")
            .or_else(|_| std::env::var("AUTOSPAWN"))
            .map_or(true, |v| v.eq_ignore_ascii_case("true"));

        let should_despawn = auto_despawn && !was_running;
        Self {
            local_config,
            test_name,
            should_despawn,
        }
    }

    pub fn disarm_and_stop(mut self) -> Result<()> {
        if self.should_despawn {
            self.should_despawn = false;
            super::stop::exec(self.local_config, self.test_name)?;
        }
        Ok(())
    }
}

impl Drop for DespawnGuard<'_> {
    fn drop(&mut self) {
        if self.should_despawn
            && let Err(e) = super::stop::exec(self.local_config, self.test_name)
        {
            warn!("Failed to stop environment (cleanup): {e}");
        }
    }
}

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
    let is_running = active.is_some();
    for environment in environments {
        let despawn_guard = DespawnGuard::new(local_config, integration, is_running);

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
        despawn_guard.disarm_and_stop()?;
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
