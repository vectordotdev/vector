use anyhow::Result;
use clap::Args;

use crate::app::Application;

/// Set the path to the Vector repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    path: String,
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        let path = app.platform.canonicalize_path(&self.path);

        let mut config = app.config.clone();
        config.repo = path;
        app.config_file.save(config);

        Ok(())
    }
}
