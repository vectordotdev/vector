extern crate vrl_cli;

use clap::Parser;
use vrl_cli::{cmd::cmd, Opts};

fn main() {
    std::process::exit(cmd(&Opts::parse()));
}
