use anyhow::Result;
use clap::Args;
use std::collections::BTreeMap;

use crate::{
    testing::runner::{LocalTestRunner, TestRunner as _},
    utils::platform,
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
        let mut args = vec!["--workspace".to_string()];

        if let Some(mut extra_args) = self.args {
            args.append(&mut extra_args);
        }

        if !args.contains(&"--features".to_string()) {
            let features = platform::default_features();
            args.extend(["--features".to_string(), features.to_string()]);
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
