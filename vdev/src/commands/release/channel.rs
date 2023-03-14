use anyhow::Result;

use crate::util::get_mode;

/// Provide the release channel (latest/nightly/custom) based on the MODE env variable.
/// This command is intended for use only within GitHub build workflows.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let channel = get_mode();

        println!("{channel}");
        Ok(())
    }
}
