use anyhow::Result;
use clap::{Args, CommandFactory};
use clap_complete::{generate, Shell};
use std::io;

use super::Cli as RootCli;

/// Display the completion file for a given shell
#[derive(Args, Debug)]
#[command()]
pub struct Cli {
    #[arg(value_enum)]
    shell: Shell,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let mut cmd = RootCli::command();
        let bin_name = cmd.get_name().to_string();
        generate(self.shell, &mut cmd, bin_name, &mut io::stdout());

        Ok(())
    }
}
