use anyhow::Result;

use crate::{app, util::CargoToml};

/// Check that all component features are set up properly
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        app::set_repo_dir()?;

        let features = extract_features()?;
        let feature_list = features.join(",");

        // cargo-hack will check each feature individually
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

fn extract_features() -> Result<Vec<String>> {
    use std::collections::HashSet;

    // Exclude default feature - already tested separately and is a cargo convention
    let excluded_defaults: HashSet<&str> = ["default"].into();

    // Exclude meta-features - aggregate features that enable multiple components together
    // Testing these would check all features at once, not in isolation
    let excluded_meta_features: HashSet<&str> = [
        "all-integration-tests",
        "all-e2e-tests",
        "vector-api-tests",
        "vector-unit-test-tests",
    ]
    .into();

    Ok(CargoToml::load()?
        .features
        .into_keys()
        .filter(|feature| {
            // Exclude utility features - internal helpers not meant to be enabled standalone
            !feature.contains("-utils")
                && !excluded_defaults.contains(feature.as_str())
                && !excluded_meta_features.contains(feature.as_str())
        })
        .collect())
}
