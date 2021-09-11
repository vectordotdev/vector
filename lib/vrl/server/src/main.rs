mod error;
mod funcs;
mod resolve;

use error::handle_err;
use funcs::function_metadata;
use resolve::resolve_vrl_input;
use structopt::StructOpt;
use warp::Filter;

#[derive(Debug, StructOpt)]
struct Opts {
    #[structopt(short = "p", long, default_value = "8080", env = "PORT")]
    port: u16,
}

#[tokio::main]
async fn main() {
    let opts = Opts::from_args();

    let cors = warp::cors()
        .allow_any_origin()
	.allow_headers(vec!["content-type"])
        .allow_methods(vec!["GET", "POST"]);

    let resolve = warp::path("resolve")
        .and(warp::post())
        .and(warp::body::json())
        .and_then(resolve_vrl_input);

    let functions = warp::path("functions")
        .and(warp::get())
        .and_then(function_metadata);

    let routes = resolve.or(functions).recover(handle_err).with(cors);

    println!("starting up the server on port {}", opts.port);

    warp::serve(routes).run(([127, 0, 0, 1], opts.port)).await;
}
