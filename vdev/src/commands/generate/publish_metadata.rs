use anyhow::Result;
use chrono::prelude::*;

use crate::{git, util};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};

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
        let version_output = format!("vector_version={version}");
        let build_desc_output = format!("vector_build_desc={build_desc}");
        let channel_output = format!("vector_release_channel={channel}");
        let cloudsmith_repo_output = format!("vector_cloudsmith_repo={cloudsmith_repo}");

        let mut output_file: Box<dyn Write> = match env::var("GITHUB_OUTPUT") {
            Ok(file_name) if !file_name.is_empty() => {
                let mut options = OpenOptions::new();
                options.write(true).append(true).create(true);
                let file = options.open(file_name)?;
                Box::new(file)
            },
            _ => Box::new(io::stdout()),
        };
        writeln!(output_file, "{version_output}")?;
        writeln!(output_file, "{build_desc_output}")?;
        writeln!(output_file, "{channel_output}")?;
        writeln!(output_file, "{cloudsmith_repo_output}")?;
        Ok(())
    }
}
