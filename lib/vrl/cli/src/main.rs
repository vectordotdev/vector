extern crate vrl_cli;

use vrl_cli::{cmd::cmd, Opts};
use structopt::StructOpt;

fn main() {
    std::process::exit(cmd(&Opts::from_args()));
}
