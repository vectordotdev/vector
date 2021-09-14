use crate::error::handle_err;
use crate::funcs::function_metadata;
use crate::resolve::resolve_vrl_input;

use structopt::StructOpt;
use warp::Filter;

#[derive(Debug, StructOpt)]
pub struct Opts {
    #[structopt(short = "p", long, default_value = "8080", env = "PORT")]
    port: u16,
}

pub async fn serve(opts: Opts) {
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

    warp::serve(routes).run(([0, 0, 0, 0], opts.port)).await
}
