use crate::cli::Opts as RootOpts;
use structopt::{StructOpt, clap::Shell};

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct Opts {
    #[structopt(name = "SHELL")]
    shell: Shell,
}

pub fn cmd(opts: &Opts) -> exitcode::ExitCode {
    RootOpts::clap().gen_completions_to("vector", opts.shell, &mut std::io::stdout());

    exitcode::OK
}
