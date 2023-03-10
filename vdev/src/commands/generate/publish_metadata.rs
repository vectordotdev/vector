use anyhow::Result;
use chrono::prelude::*;

use crate::{git, util};

/// Setting necessary metadata for our publish workflow in CI
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Generate the Vector version and build description.
        let version = util::read_version()?;

        let git_sha = git::get_git_sha()?;
        let current_date = Local::today().naive_local().to_string();
        let build_desc = format!("{git_sha} {current_date}");

        // Figure out what our release channel is.
        let channel = util::release_channel()?;

        // Depending on the channel, this influences which Cloudsmith repository we publish to.
        let cloudsmith_repo = match channel.as_ref() {
            "nightly" => "vector-nightly",
            _ => "vector"
        };

        // Set the output variables
        println!("vector_version={version}" );
        println!("vector_build_desc={build_desc}");
        println!("vector_release_channel={channel}");
        println!("vector_cloudsmith_repo={cloudsmith_repo}");

        Ok(())
    }
}