use clap::Parser;
use vrl::cli::{Opts, cmd::cmd};

fn main() {
    let functions = vector_vrl_all::all_vrl_functions();
    std::process::exit(cmd(&Opts::parse(), functions));
}
