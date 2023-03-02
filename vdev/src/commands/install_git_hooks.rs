use anyhow::{Result, Ok};
use std::process::Command;
use crate::git;
use crate::app::CommandExt as _;
// use std::{env, path::PathBuf};

/// Install a Git commit message hook that verifies
/// that all commits have been signed off.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// The mode argument. Can be used to control which hook(s) are installed,
    /// with the default being to install all available hooks.
    mode: Option<String>
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mode = self.mode.unwrap_or_else(|| "all".to_string());
        let git_dir = git::get_git_dir()?;

        // Create a new directory named hooks in the .git directory if it
        // doesn't already exist.
        let mut command = Command::new("mkdir");
        command.in_repo();
        command.args(["-p", &format!("{git_dir}/hooks")]);

        command.check_run()?;

        // Copy the script scripts/signoff-git-hook.sh to the
        // .git/hooks/commit-msg file
        if mode == "all" || mode == "signoff" {
            command = Command::new("cp");
            command.args(["scripts/signoff-git-hook.sh", &format!("{git_dir}/hooks/commit-msg")]);
            command.check_run()?;
            println!("Copied scripts/signoff-git-hook.sh to {git_dir}/hooks/commit-msg");
        }
        Ok(())
    }
}
