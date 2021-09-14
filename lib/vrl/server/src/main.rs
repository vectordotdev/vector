use vrl_server::server::{serve, Opts};

use structopt::StructOpt;

#[tokio::main]
async fn main() {
    let opts = Opts::from_args();

    serve(opts);
}
