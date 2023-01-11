use std::process::{Command, Stdio};

use anyhow::{bail, Context as _, Result};

/// Determine the appropriate release channel (nightly or latest) based on Git HEAD.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut command = Command::new("git");
        command.args(["describe", "--exact-match", "--tags", "HEAD"]);
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
        let channel = match command.spawn().context("Could not execute `git`")?.wait() {
            Ok(status) if status.success() => "latest",
            Ok(_) => "nightly",
            Err(error) => bail!("Could not wait for `git`: {}", error),
        };
        println!("{channel}");
        Ok(())
    }
}
