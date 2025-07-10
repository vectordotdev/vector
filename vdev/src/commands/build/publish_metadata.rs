use anyhow::Result;
use chrono::prelude::*;

use crate::{git, util};
use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write};

/// Setting necessary metadata for our publish workflow in CI.
///
/// Responsible for setting necessary metadata for our publish workflow in CI. Computes the Vector
/// version (from Cargo.toml), the release channel (nightly vs release), and more. All of this
/// information is emitted in a way that sets native outputs on the GitHub Actions workflow step
/// running the script, which can be passed on to subsequent jobs/steps.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        // Generate the Vector version and build description.
        let version = util::get_version()?;

        let git_sha = git::get_git_sha()?;
        let current_date = Local::now().naive_local().to_string();
        let build_desc = format!("{git_sha} {current_date}");

        // Figure out what our release channel is.
        let channel = util::get_channel();

        let mut output_file: Box<dyn Write> = match env::var("GITHUB_OUTPUT") {
            Ok(file_name) if !file_name.is_empty() => {
                let file = OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(file_name)?;
                Box::new(file)
            }
            _ => Box::new(io::stdout()),
        };
        writeln!(output_file, "vector_version={version}")?;
        writeln!(output_file, "vector_build_desc={build_desc}")?;
        writeln!(output_file, "vector_release_channel={channel}")?;
        Ok(())
    }
}
