use anyhow::Result;

use crate::{app, utils::cargo::CargoToml};

/// Check that all component features are set up properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        let features = extract_features()?;
        let feature_list = features.join(",");

        // cargo-hack will check each feature individually with --no-default-features.
        app::exec(
            "cargo",
            [
                "hack",
                "check",
                "--tests",
                "--bin",
                "vector",
                "--each-feature",
                "--include-features",
                &feature_list,
            ],
            true,
        )
    }
}

// Exclude default feature - already tested separately and is a cargo convention
const EXCLUDED_DEFAULTS: &[&str] = &["default"];

// Exclude meta-features - aggregate features that enable multiple components together.
// Testing these would check all features at once, not in isolation
const EXCLUDED_META_FEATURES: &[&str] = &[
    "all-integration-tests",
    "all-e2e-tests",
    "vector-api-tests",
    "vector-unit-test-tests",
];

fn extract_features() -> Result<Vec<String>> {
    Ok(CargoToml::load()?
        .features
        .into_keys()
        .filter(|feature| {
            // Exclude utility features - internal helpers not meant to be enabled standalone
            !feature.contains("-utils")
                && !EXCLUDED_DEFAULTS.contains(&feature.as_str())
                && !EXCLUDED_META_FEATURES.contains(&feature.as_str())
        })
        .collect())
}
