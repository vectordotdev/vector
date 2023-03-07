use std::env;

use anyhow::{bail, Result};

use crate::{app, util};

/// Compute the release version of Vector.
#[derive(clap::Args, Debug)]
pub(super) struct Cli {}

impl Cli {
    pub(super) fn exec(self) -> Result<()> {
        app::set_repo_dir()?;
        let mut version = env::var("VERSION").or_else(|_| util::read_version())?;
        let channel = env::var("CHANNEL")
            .or_else(|_| env::var("MODE").or_else(|_| util::release_channel().map(Into::into)))?;

        if channel == "latest" {
            let head = util::git_head()?;
            if !head.status.success() {
                let error = String::from_utf8_lossy(&head.stderr);
                bail!("Error running `git describe`:\n{error}");
            }
            let tag = String::from_utf8_lossy(&head.stdout).trim().to_string();
            if tag != format!("v{version}") {
                bail!("On latest release channel and tag {tag:?} is different from Cargo.toml {version:?}. Aborting");
            }
        } else if channel == "custom" {
            let short_hash_out = util::git_short_hash()?;
            if !short_hash_out.status.success() {
                let error = String::from_utf8_lossy(&short_hash_out.stderr);
                bail!("Error getting short hash running `git rev-parse`:\n{error}");
            }
            let short_hash = String::from_utf8_lossy(&short_hash_out.stdout)
                .trim()
                .to_string();

            // use '.' instead of '-' or '_' to avoid issues with rpm and deb package naming
            // format requirements.
            version = format!("{version}.custom.{short_hash}");
        }

        println!("{version}");
        Ok(())
    }
}
