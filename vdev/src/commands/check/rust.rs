use anyhow::{Result, bail};
use std::{ffi::OsString, fs};

use crate::{
    app,
    utils::{command::ChainArgs as _, git, paths},
};

/// Check the Rust code for errors
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(long, default_value_t = true)]
    clippy: bool,

    /// Exact feature set to check. Include `default` to enable the default features.
    #[arg(short = 'F', long, value_delimiter = ',')]
    features: Vec<String>,

    #[arg(long)]
    no_default_features: bool,

    #[arg(long)]
    fix: bool,
}

#[derive(strum::Display, strum::AsRefStr, Clone, Copy, Debug)]
#[strum(serialize_all = "lowercase")]
enum Tool {
    Clippy,
    Check,
}

impl Cli {
    /// Build the argument vector for cargo invocation.
    fn build_args(&self, tool: Tool) -> Vec<OsString> {
        let pre_args = if self.fix {
            vec!["--fix"]
        } else {
            Vec::default()
        };

        let features: Vec<&str> = self
            .features
            .iter()
            .map(String::as_str)
            .filter(|f| !f.is_empty())
            .collect();

        let feature_args: Vec<String> = if !features.is_empty() {
            vec![
                "--no-default-features".to_string(),
                "--features".to_string(),
                features.join(","),
            ]
        } else if self.no_default_features {
            vec!["--no-default-features".to_string()]
        } else {
            vec!["--all-features".to_string()]
        };

        [tool.as_ref(), "--workspace", "--all-targets"]
            .chain_args(feature_args)
            .chain_args(pre_args)
    }

    pub fn exec(self) -> Result<()> {
        let lock_file = paths::find_repo_root()?.join("Cargo.lock");
        let lock_before = fs::read(&lock_file)?;

        let tool = if self.clippy {
            Tool::Clippy
        } else {
            Tool::Check
        };

        app::exec("cargo", self.build_args(tool), true)?;

        let lock_after = fs::read(&lock_file)?;
        if lock_before != lock_after {
            bail!(
                "Cargo.lock was modified by `cargo {tool}`. Please commit the updated Cargo.lock."
            );
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(features: &[&str], no_default_features: bool) -> Vec<OsString> {
        Cli {
            clippy: true,
            features: features.iter().map(ToString::to_string).collect(),
            no_default_features,
            fix: false,
        }
        .build_args(Tool::Clippy)
    }

    #[test]
    fn builds_feature_args() {
        let cases: &[(&[&str], bool, &[&str])] = &[
            (&[], false, &["--all-features"]),
            (&[], true, &["--no-default-features"]),
            (
                &["sources-file"],
                false,
                &["--no-default-features", "--features", "sources-file"],
            ),
            (
                &["default", "sources-file"],
                false,
                &[
                    "--no-default-features",
                    "--features",
                    "default,sources-file",
                ],
            ),
        ];

        for (features, no_default_features, feature_args) in cases {
            let expected = ["clippy", "--workspace", "--all-targets"]
                .into_iter()
                .chain(feature_args.iter().copied())
                .map(OsString::from)
                .collect::<Vec<_>>();
            assert_eq!(
                args(features, *no_default_features),
                expected,
                "features={features:?}, no_default_features={no_default_features}"
            );
        }
    }
}
