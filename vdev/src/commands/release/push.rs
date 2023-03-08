use std::env;
// use std::process::Command;

use anyhow::Result;
use clap::Args;

use crate::util;


// use crate::app::CommandExt as _;
// use crate::platform;

/// Pushes new versions produced by `make release` to the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let _version = env::var("VECTOR_VERSION").or_else(|_| util::read_version())?;
        
        Ok(())
    }
}
