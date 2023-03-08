use anyhow::Result;
use std::env;

use crate::util;

/// Determine the appropriate release channel (latest, nightly) based on Git HEAD.
/// If the env var "MODE", is set, that is used instead.
// This script is used across various release scripts to determine where distribute archives,
// packages, etc.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let channel = env::var("MODE").or_else(|_| util::release_channel().map(Into::into))?;

        println!("{channel}");
        Ok(())
    }
}
