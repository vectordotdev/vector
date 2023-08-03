use std::path::PathBuf;

use anyhow::Result;
use clap::Args;

use crate::features;

/// Extract the set of features required to run a given config
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    config: PathBuf,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        for feature in features::load_and_extract(&self.config)? {
            println!("{feature}");
        }
        Ok(())
    }
}
