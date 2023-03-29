use anyhow::{Result, Ok};
use std::process::Command;
use std::env;
use crate::app::CommandExt as _;
use crate::util;

/// Uploads target/artifacts to GitHub releases
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let version = env::var("VECTOR_VERSION").or_else(|_| util::read_version())?;
        let mut command = Command::new("gh");
        command.in_repo();
        command.args(["release",
                "--repo",
                "vectordotdev/vector",
                "create",
                &format!("v{version}"),
                "--title",
                &format!("v{version}"),
                "--notes",
                &format!("[View release notes](https://vector.dev/releases/{version})"),
                "target/artifacts/*"]);
        command.check_run()?;
        Ok(())
    }
}
