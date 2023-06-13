use clap::Parser;
use vrl::cli::{cmd::cmd, Opts};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut functions = vrl::stdlib::all();
    functions.extend(vector_vrl_functions::all());

    cmd(&Opts::parse(), functions)
}
