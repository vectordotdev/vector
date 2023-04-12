extern crate vrl_cli;

use clap::Parser;
use vrl_cli::{cmd::cmd, Opts};

fn main() {
    let mut functions = vrl_stdlib::all();
    functions.extend(vector_vrl_functions::all());

    std::process::exit(cmd(&Opts::parse(), functions));
}
