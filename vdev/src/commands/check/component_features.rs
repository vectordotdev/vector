use std::env;

use anyhow::Result;

use crate::{app, util::CargoToml};

const CARGO: &str = "cargo";
const BASE_ARGS: [&str; 5] = [
    "check",
    "--tests",
    "--bin",
    "vector",
    "--no-default-features",
];
const PARALLEL_ARGS: [&str; 7] = [
    "--group",
    "--verbose",
    "--retries",
    "2",
    "scripts/check-one-feature",
    "{}",
    ":::",
];

/// Check that all component features are set up properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    #[allow(clippy::dbg_macro)]
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        let features = extract_features()?;

        // Prime the pump to build most of the artifacts
        app::exec(CARGO, BASE_ARGS, true)?;
        app::exec(
            CARGO,
            BASE_ARGS.into_iter().chain(["--features", "default"]),
            true,
        )?;
        app::exec(
            CARGO,
            BASE_ARGS
                .into_iter()
                .chain(["--features", "all-integration-tests"]),
            true,
        )?;

        // The feature builds already run in parallel below, so don't overload the parallelism
        env::set_var("CARGO_BUILD_JOBS", "1");

        app::exec(
            "parallel",
            PARALLEL_ARGS
                .into_iter()
                .chain(features.iter().map(String::as_str)),
            true,
        )
    }
}

fn extract_features() -> Result<Vec<String>> {
    Ok(CargoToml::load()?
        .features
        .into_keys()
        .filter(|feature| {
            !feature.contains("-utils")
                && feature != "default"
                && feature != "all-integration-tests"
        })
        .collect())
}
