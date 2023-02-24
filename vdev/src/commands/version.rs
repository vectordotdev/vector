use std::env;

use anyhow::{bail, Result};

use crate::{app, util};

/// Compute the release version of Vector.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        let version = env::var("VERSION").or_else(|_| util::read_version())?;
        let channel = env::var("CHANNEL").or_else(|_| util::release_channel().map(Into::into))?;

        if channel == "latest" {
            let head = util::git_head()?;
            if !head.status.success() {
                let error = String::from_utf8_lossy(&head.stderr);
                bail!("Error running `git describe`:\n{error}");
            }
            let tag = String::from_utf8_lossy(&head.stdout);
            if tag != format!("v{version}") {
                bail!("On latest release channel and tag {tag:?} is different from Cargo.toml {version:?}. Aborting");
            }
        }

        println!("{version}");
        Ok(())
    }
}
