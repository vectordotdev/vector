use structopt::StructOpt;
use vrl_cli::{run, Opts};

fn main() {
    let status = match run(Opts::from_args()) {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("{}", err);
            1
        }
    };

    std::process::exit(status);
}
