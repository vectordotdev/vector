use anyhow::Result;
use clap::Args;

use crate::app::Application;

/// Set the target Datadog org
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    name: String,
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        let mut config = app.config.clone();
        config.org = self.name.to_string();
        app.config_file.save(config);

        Ok(())
    }
}
