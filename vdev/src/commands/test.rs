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

    /// Features to activate (comma-separated, or set FEATURES env var)
    #[arg(short = 'F', long, value_delimiter = ',', env = "FEATURES")]
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

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut selected_features: Vec<String> = self
            .features
            .into_iter()
            .filter(|f| !f.is_empty())
            .collect();

        if let Some(config) = &self.config {
            selected_features.extend(features::load_and_extract(config)?);
            selected_features.sort();
            selected_features.dedup();
        }

        let mut args = vec!["--workspace".to_string()];

        if self.no_default_features || self.config.is_some() {
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

    use super::Cli;

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
    fn propagates_config_loading_errors_before_starting_tests() {
        let error = Cli {
            args: None,
            env: None,
            features: Vec::new(),
            config: Some(PathBuf::from("missing-vector-config.yaml")),
            no_default_features: false,
        }
        .exec()
        .expect_err("missing configuration must fail");

        assert!(error.to_string().contains("failed to read"));
    }
}
