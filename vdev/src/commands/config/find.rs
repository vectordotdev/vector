use clap::Args;

use crate::app::Application;

/// Locate the config file
#[derive(Args, Debug)]
#[command()]
pub struct Cli {}

impl Cli {
    pub fn exec(&self, app: &Application) {
        app.display(format!("{}", app.config_file.path().display()));
    }
}
