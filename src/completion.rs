#![allow(missing_docs)]
use clap::{CommandFactory, Parser};
use clap_complete::{Shell, generate};
use std::io;

use crate::cli::Opts as RootCli;

#[derive(Parser, Debug, Clone)]
#[command(rename_all = "kebab-case")]
pub struct Opts {
    /// Shell to generate completion for
    #[clap(value_enum)]
    shell: Shell,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    let mut cmd = RootCli::command();
    let bin_name = cmd.get_name().to_string();

    generate(opts.shell, &mut cmd, bin_name, &mut io::stdout());

    exitcode::OK
}
