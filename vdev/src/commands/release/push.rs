use std::env;

use anyhow::Result;
use clap::Args;

use crate::util;
use crate::git;

/// Pushes new versions produced by `make release` to the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = env::var("VECTOR_VERSION").or_else(|_| util::read_version())?;
        let version_minor = version.split('.')
            .take(2)
            .collect::<Vec<&str>>()
            .join(".");

        let current_branch = git::current_branch()?;
        println!("Preparing the branch and the tag...");
        git::checkout_branch(&format!("v{version_minor}"))?;
        git::merge_branch(&current_branch)?;
        git::tag_version(&format!("v{version}"))?;

        println!("Pushing the branch and the tag...");
        git::push_branch(&format!("v{version_minor}"))?;
        git::push_branch(&format!("v{version}"))?;

        Ok(())
    }
}
