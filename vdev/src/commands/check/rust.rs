use anyhow::Result;
use std::ffi::OsString;

use crate::{app, git, util::ChainArgs as _};

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
    /// Build args with specific workspace target
    fn build_args_with_target(&self, workspace_arg: &str) -> Vec<OsString> {
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
                self.features.join(",").to_string(),
            ]
        };

        [tool, workspace_arg, "--all-targets"]
            .chain_args(feature_args)
            .chain_args(pre_args)
            .chain_args(clippy_args)
    }

    /// Build args for checking Vector workspace
    fn build_vector_args(&self) -> Vec<OsString> {
        self.build_args_with_target("--workspace")
    }

    /// Build args for checking vdev crate
    fn build_vdev_args(&self) -> Vec<OsString> {
        self.build_args_with_target("--manifest-path=vdev/Cargo.toml")
    }

    pub fn exec(self) -> Result<()> {
        app::exec("cargo", self.build_vector_args(), true)?;
        app::exec("cargo", self.build_vdev_args(), true)?;

        // If --fix was used, check for changes and commit them.
        if self.fix {
            let has_changes = !git::get_modified_files()?.is_empty();
            if has_changes {
                git::commit("chore(vdev): apply vdev rust check fixes")?;
            }
        }

        Ok(())
    }
}
