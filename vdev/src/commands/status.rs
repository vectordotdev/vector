use anyhow::Result;
use clap::Args;

use crate::app::Application;

/// Show information about the current environment
#[derive(Args, Debug)]
#[command(hide = true)]
pub struct Cli {}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        app.display(format!("Branch: {}", app.repo.git.current_branch()?));
        app.display(format!("Changed: {:?}", app.repo.git.changed_files()?));

        Ok(())
    }
}
