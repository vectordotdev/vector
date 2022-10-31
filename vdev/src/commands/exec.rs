use anyhow::Result;
use clap::Args;

use crate::app::Application;

/// Execute a command within the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl Cli {
    pub fn exec(&self, app: &Application) -> Result<()> {
        let mut command = app.repo.command(&self.args[0]);
        command.args(&self.args[1..]);

        let status = command.status()?;
        app.exit(status.code().unwrap());

        Ok(())
    }
}
