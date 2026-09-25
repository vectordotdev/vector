use std::iter::once;

use anyhow::{Result, bail};

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
        clear_coverage();
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

    // Consolidate per-environment coverage files into the canonical lcov.info
    // so callers get a single, predictable output path regardless of how many
    // environments ran.
    if coverage {
        merge_coverage(&ran_environments)?;
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

pub(crate) fn clear_coverage() {
    let coverage_dir = local_coverage_output_dir();
    std::fs::remove_file(coverage_dir.join(coverage_filename(None))).ok();
}

pub(crate) fn merge_coverage(environments: &[String]) -> Result<()> {
    if environments.is_empty() {
        return Ok(());
    }

    let coverage_dir = local_coverage_output_dir();
    let merged_path = coverage_dir.join(coverage_filename(None));
    let mut merged = String::new();

    for environment in environments {
        let coverage_path = coverage_dir.join(coverage_filename(Some(environment)));
        match std::fs::read_to_string(&coverage_path) {
            Ok(contents) => {
                merged.push_str(&normalize_coverage_paths(&contents));
                std::fs::remove_file(&coverage_path).ok();
            }
            Err(e) => warn!(
                "Could not read coverage file {}: {e}",
                coverage_path.display()
            ),
        }
    }

    if !merged.is_empty() {
        std::fs::write(&merged_path, merged)?;
        info!(
            "Wrote coverage for {} environment(s) to {}",
            environments.len(),
            merged_path.display()
        );
    }

    Ok(())
}

fn normalize_coverage_paths(contents: &str) -> String {
    contents.replace("SF:/home/vector/", "SF:")
}

#[cfg(test)]
mod tests {
    use super::normalize_coverage_paths;

    #[test]
    fn normalizes_container_source_paths() {
        assert_eq!(
            normalize_coverage_paths("SF:/home/vector/src/main.rs\nDA:1,1\n"),
            "SF:src/main.rs\nDA:1,1\n"
        );
    }
}
