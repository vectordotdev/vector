use crate::app::CommandExt as _;
use crate::util;
use anyhow::{Ok, Result};
use std::process::Command;

/// Uploads target/artifacts to GitHub releases
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = util::get_version()?;
        let mut command = Command::new("gh");
        command.in_repo();
        command.args([
            "release",
            "--repo",
            "vectordotdev/vector",
            "create",
            &format!("v{version}"),
            "--title",
            &format!("v{version}"),
            "--notes",
            &format!("[View release notes](https://vector.dev/releases/{version})"),
            "target/artifacts/*",
        ]);
        command.check_run()?;
        Ok(())
    }
}
