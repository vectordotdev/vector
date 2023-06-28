use clap::Parser;
use vrl::cli::{cmd::cmd, Opts};

fn main() {
    let mut functions = vrl::stdlib::all();
    functions.extend(vector_vrl_functions::all());

    std::process::exit(cmd(&Opts::parse(), functions));
}
