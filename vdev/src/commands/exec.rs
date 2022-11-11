use anyhow::{bail, Result};
use clap::Args;

use crate::app;

/// Execute a command within the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl Cli {
    pub fn exec(&self) -> Result<()> {
        let mut command = app::construct_command(&self.args[0]);
        command.args(&self.args[1..]);

        let status = command.status()?;
        if !status.success() {
            bail!("failed with exit code: {}", status.code().unwrap_or(1));
        }

        Ok(())
    }
}
