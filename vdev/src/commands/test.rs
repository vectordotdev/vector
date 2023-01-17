use anyhow::Result;
use clap::Args;
use std::collections::BTreeMap;

use crate::platform;
use crate::testing::{config::RustToolchainConfig, runner::get_agent_test_runner};

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

fn parse_env(env: Vec<String>) -> BTreeMap<String, String> {
    env.into_iter()
        .map(|entry| {
            let split = entry.split_once('=');
            #[allow(clippy::map_unwrap_or)]
            split
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .unwrap_or_else(|| (entry, String::new()))
        })
        .collect()
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let toolchain_config = RustToolchainConfig::parse()?;
        let runner = get_agent_test_runner(self.container, toolchain_config.channel);

        let mut args = vec!["--workspace".to_string()];
        if let Some(extra_args) = &self.args {
            args.extend(extra_args.clone());

            if !(self.container || extra_args.contains(&"--features".to_string())) {
                let features = platform::default_features();
                args.extend(["--features".to_string(), features.to_string()]);
            }
        }

        runner.test(&parse_env(self.env.unwrap_or_default()), &args)
    }
}
