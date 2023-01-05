use anyhow::Result;
use clap::Args;

use crate::{app, config};

/// Set the target Datadog org
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    name: String,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut config = app::config().clone();
        config.org = self.name.to_string();
        config::save(config)?;

        Ok(())
    }
}
