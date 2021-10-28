use crate::cli::RootOpts;
use structopt::{StructOpt, clap::Shell};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    #[structopt(default_value = "bash", possible_values = &possible_shell_values())]
    shell: Shell,
}

fn possible_shell_values() -> [&'static str; 5] {
    Shell::variants()
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    RootOpts::clap().gen_completions(env!("CARGO_PKG_NAME"), opts.shell, "target");

    exitcode::OK
}
