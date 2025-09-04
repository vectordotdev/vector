use anyhow::Result;
use clap::Args;

use crate::app::VDevCommand;

/// Execute a command within the repository
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    args: Vec<String>,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let status = VDevCommand::new(&self.args[0])
            .in_repo()
            .args(&self.args[1..])
            .run()?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
