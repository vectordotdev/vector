extern crate remap_cli;

use remap_cli::{cmd::cmd, Opts};
use structopt::StructOpt;

fn main() {
    std::process::exit(cmd(&Opts::from_args()));
}
