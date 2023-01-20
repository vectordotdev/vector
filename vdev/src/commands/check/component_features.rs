use std::{collections::BTreeMap, env, fs::File, io::Read as _};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::app;

#[derive(Deserialize)]
struct CargoToml {
    features: BTreeMap<String, Value>,
}

const CARGO: &str = "cargo";
const BASE_ARGS: [&str; 5] = [
    "check",
    "--tests",
    "--bin",
    "vector",
    "--no-default-features",
];

/// Check that all component features are set up properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    #[allow(clippy::dbg_macro)]
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        let features = extract_features()?.join(",");

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
            [
                "--group",
                "--verbose",
                "--retries",
                "2",
                "scripts/check-one-feature",
                "{}",
                ":::",
                &features,
            ],
            true,
        )
    }
}

fn extract_features() -> Result<Vec<String>> {
    let mut text = String::new();
    File::open("Cargo.toml")
        .context("Could not open `Cargo.toml`")?
        .read_to_string(&mut text)
        .context("Could not read `Cargo.toml`")?;
    Ok(toml::from_str::<CargoToml>(&text)
        .context("Could not parse `Cargo.toml`")?
        .features
        .into_keys()
        .filter(|feature| {
            !feature.contains("-utils")
                && feature != "default"
                && feature != "all-integration-tests"
        })
        .collect())
}
