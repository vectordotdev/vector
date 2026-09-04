use anyhow::Result;
use clap::Args;
use std::{collections::BTreeMap, path::PathBuf};

use crate::{
    testing::runner::{LocalTestRunner, TestRunner as _},
    utils::features,
};

/// Execute tests
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Extra test command arguments
    args: Option<Vec<String>>,

    /// Environment variables in the form KEY[=VALUE]
    #[arg(short, long)]
    env: Option<Vec<String>>,

    /// Features to activate explicitly (comma-separated, additive with FEATURES)
    #[arg(short = 'F', long, value_delimiter = ',')]
    features: Vec<String>,

    /// Derive the minimum feature set from a configuration file
    #[arg(long)]
    config: Option<PathBuf>,

    #[arg(long)]
    no_default_features: bool,
}

fn parse_env(env: Vec<String>) -> BTreeMap<String, Option<String>> {
    env.into_iter()
        .map(|entry| {
            #[allow(clippy::map_unwrap_or)] // Can't use map_or due to borrowing entry
            entry
                .split_once('=')
                .map(|(k, v)| (k.to_owned(), Some(v.to_owned())))
                .unwrap_or_else(|| (entry, None))
        })
        .collect()
}

fn select_features(
    explicit: &[String],
    environment: Option<&str>,
    derived: Vec<String>,
    config_selected: bool,
) -> (Vec<String>, bool) {
    let mut ignored_environment_default = false;
    let mut selected: Vec<_> = explicit
        .iter()
        .filter(|feature| !feature.is_empty())
        .cloned()
        .collect();
    if let Some(environment) = environment {
        for feature in environment
            .split(|character: char| character == ',' || character.is_ascii_whitespace())
            .filter(|feature| !feature.is_empty())
        {
            if config_selected && feature == "default" {
                ignored_environment_default = true;
            } else {
                selected.push(feature.to_owned());
            }
        }
    }
    selected.extend(derived);
    selected.sort();
    selected.dedup();

    (selected, ignored_environment_default)
}

impl Cli {
    fn resolve_features(&self) -> Result<Vec<String>> {
        let config_selected = self.config.is_some();
        let derived_features = self
            .config
            .as_deref()
            .map(features::load_and_extract)
            .transpose()?
            .unwrap_or_default();
        let environment_features = std::env::var("FEATURES").ok();
        let (selected_features, ignored_environment_default) = select_features(
            &self.features,
            environment_features.as_deref(),
            derived_features,
            config_selected,
        );
        if ignored_environment_default {
            warn!("Ignoring `default` from FEATURES because --config uses --no-default-features");
        }

        Ok(selected_features)
    }

    pub fn exec(self) -> Result<()> {
        let config_selected = self.config.is_some();
        let selected_features = self.resolve_features()?;

        let mut args = vec!["--workspace".to_string()];

        if self.no_default_features || config_selected {
            args.push("--no-default-features".to_string());
        }
        if !selected_features.is_empty() {
            args.extend(["--features".to_string(), selected_features.join(",")]);
        }

        if let Some(mut extra_args) = self.args {
            args.append(&mut extra_args);
        }

        LocalTestRunner.test(
            &parse_env(self.env.unwrap_or_default()),
            &BTreeMap::default(),
            None,
            &args,
            false, // Don't pre-build Vector for direct test runs
            false,
            None,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use clap::Parser;

    use super::{Cli, select_features};

    #[derive(Parser)]
    struct TestCli {
        #[command(flatten)]
        test: Cli,
    }

    #[test]
    fn accepts_a_config_before_the_test_filter() {
        let cli =
            TestCli::try_parse_from(["vdev-test", "--config", "vector.yaml", "test_some_function"])
                .expect("test arguments must parse")
                .test;

        assert_eq!(cli.config, Some(PathBuf::from("vector.yaml")));
        assert_eq!(cli.args, Some(vec!["test_some_function".to_string()]));
    }

    #[test]
    fn resolve_features_propagates_config_loading_errors() {
        let error = Cli {
            args: None,
            env: None,
            features: Vec::new(),
            config: Some(PathBuf::from("missing-vector-config.yaml")),
            no_default_features: false,
        }
        .resolve_features()
        .expect_err("missing configuration must fail");

        assert!(error.to_string().contains("failed to read"));
    }

    #[test]
    fn config_ignores_only_environment_default() {
        let (features, ignored_default) = select_features(
            &["default".to_string(), "sinks-console".to_string()],
            Some("default,rdkafka/dynamic-linking"),
            vec!["sources-file".to_string()],
            true,
        );

        assert!(ignored_default);
        assert_eq!(
            features,
            [
                "default",
                "rdkafka/dynamic-linking",
                "sinks-console",
                "sources-file",
            ]
        );
    }
}
