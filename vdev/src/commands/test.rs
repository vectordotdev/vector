use anyhow::Result;
use clap::Args;
use std::collections::BTreeMap;

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

impl Cli {
    pub fn exec(self) -> Result<()> {
        let toolchain_config = RustToolchainConfig::parse()?;
        let runner = get_agent_test_runner(self.container, toolchain_config.channel);

        let mut env_vars = BTreeMap::new();
        if let Some(extra_env_vars) = &self.env {
            for entry in extra_env_vars {
                if let Some((key, value)) = entry.split_once('=') {
                    env_vars.insert(key.to_string(), value.to_string());
                } else {
                    env_vars.insert(entry.to_string(), String::new());
                }
            }
        }

        let mut args = vec!["--workspace".to_string()];
        if let Some(extra_args) = &self.args {
            args.extend(extra_args.clone());

            if !(self.container || extra_args.contains(&"--features".to_string())) {
                if cfg!(windows) {
                    args.extend(["--features".to_string(), "default-msvc".to_string()]);
                } else {
                    args.extend(["--features".to_string(), "default".to_string()]);
                }
            }
        }

        runner.test(&env_vars, &args)
    }
}
