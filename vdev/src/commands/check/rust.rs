use anyhow::Result;
use std::ffi::OsString;

use crate::{
    app,
    utils::{command::ChainArgs as _, git},
};

/// Check the Rust code for errors
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(long, default_value_t = true)]
    clippy: bool,

    #[arg(value_name = "FEATURE")]
    features: Vec<String>,

    #[arg(long)]
    fix: bool,
}

impl Cli {
    /// Build the argument vector for cargo invocation.
    fn build_args(&self) -> Vec<OsString> {
        let tool = if self.clippy { "clippy" } else { "check" };

        let pre_args = if self.fix {
            vec!["--fix"]
        } else {
            Vec::default()
        };

        let clippy_args = if self.clippy {
            vec!["--", "-D", "warnings"]
        } else {
            Vec::default()
        };

        let feature_args = if self.features.is_empty() {
            vec!["--all-features".to_string()]
        } else {
            vec![
                "--no-default-features".to_string(),
                "--features".to_string(),
                self.features.join(",").clone(),
            ]
        };

        [tool, "--workspace", "--all-targets"]
            .chain_args(feature_args)
            .chain_args(pre_args)
            .chain_args(clippy_args)
    }

    pub fn exec(self) -> Result<()> {
        app::exec("cargo", self.build_args(), true)?;

        // If --fix was used, check for changes and commit them.
        if self.fix {
            let has_changes = !git::get_modified_files()?.is_empty();
            if has_changes {
                app::exec("cargo", ["fmt", "--all"], true)?;
                git::commit("chore(vdev): apply vdev rust check fixes")?;
            }
        }

        Ok(())
    }
}
