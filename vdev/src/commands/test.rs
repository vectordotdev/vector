use anyhow::Result;
use clap::Args;
use std::collections::BTreeMap;

use crate::platform;
use crate::testing::runner::get_agent_test_runner;

/// Execute tests
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    /// Extra test command arguments
    args: Option<Vec<String>>,

    /// Whether to run tests in a container
    #[arg(short = 'C', long)]
    container: bool,

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
        let runner = get_agent_test_runner(self.container)?;

        let mut args = vec!["--workspace".to_string()];

        if let Some(mut extra_args) = self.args {
            args.append(&mut extra_args);
        }

        if !args.contains(&"--features".to_string()) {
            let features = platform::default_features();
            args.extend(["--features".to_string(), features.to_string()]);
        }

        runner.test(
            &parse_env(self.env.unwrap_or_default()),
            &BTreeMap::default(),
            None,
            &args,
            "",
        )
    }
}
