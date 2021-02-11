extern crate vrl_cli;

use structopt::StructOpt;
use vrl_cli::{cmd::cmd, Opts};

fn main() {
    std::process::exit(cmd(&Opts::from_args()));
}
