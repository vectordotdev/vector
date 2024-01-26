use anyhow::Result;
use clap::Args;

use crate::app;
use crate::git;
use itertools::Itertools;

/// Pushes new versions produced by `make release` to the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = app::version()?;
        let version_minor = version.split('.').take(2).join(".");

        let current_branch = git::current_branch()?;
        let branch_name = format!("v{version_minor}");
        let tag_name = format!("v{version}");
        println!("Preparing the branch and the tag...");
        git::checkout_or_create_branch(&branch_name)?;
        git::merge_branch(&current_branch)?;
        git::tag_version(&tag_name)?;

        println!("Pushing the branch and the tag...");
        git::push_branch(&branch_name)?;
        git::push_branch(&tag_name)?;

        Ok(())
    }
}
