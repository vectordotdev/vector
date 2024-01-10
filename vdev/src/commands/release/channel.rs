use anyhow::Result;

use crate::util::get_channel;

/// Provide the release channel (release/nightly/custom).
/// This command is intended for use only within GitHub build workflows.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let channel = get_channel();

        println!("{channel}");
        Ok(())
    }
}
