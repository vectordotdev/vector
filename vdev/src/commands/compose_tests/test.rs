use std::iter::once;

use anyhow::{Result, bail};

use crate::testing::{
    config::ComposeTestConfig,
    integration::{ComposeTest, ComposeTestLocalConfig},
    runner::{LOCAL_COVERAGE_OUTPUT_DIR, coverage_filename},
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

    let mut ran_environments: Vec<String> = Vec::new();
    for environment in environments {
        ComposeTest::generate(local_config, integration, environment, retries, coverage)?
            .test(args.to_owned())?;
        if coverage {
            ran_environments.push(environment.clone());
        }
    }

    // Consolidate per-environment coverage files into the canonical lcov.info
    // so callers get a single, predictable output path regardless of how many
    // environments ran.
    if coverage && !ran_environments.is_empty() {
        let coverage_dir = std::path::Path::new(LOCAL_COVERAGE_OUTPUT_DIR);
        let merged_path = coverage_dir.join(coverage_filename(None));
        // Remove any stale lcov.info from a previous run so callers never pick
        // up outdated data if the merge below fails to read a per-env file.
        let _ = std::fs::remove_file(&merged_path);
        let mut merged = String::new();
        for env_name in &ran_environments {
            let env_file = coverage_dir.join(coverage_filename(Some(env_name)));
            match std::fs::read_to_string(&env_file) {
                Ok(contents) => {
                    merged.push_str(&contents);
                    let _ = std::fs::remove_file(&env_file);
                }
                Err(e) => {
                    warn!("Could not read coverage file {}: {e}", env_file.display());
                }
            }
        }
        if !merged.is_empty() {
            std::fs::write(&merged_path, merged)?;
            info!(
                "Wrote coverage for {} environment(s) to {}",
                ran_environments.len(),
                merged_path.display()
            );
        }
    }

    Ok(())
}
