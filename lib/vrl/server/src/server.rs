use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use crate::error::handle_err;
use crate::funcs::function_metadata;
use crate::resolve::resolve_vrl_input;

use warp::Filter;

pub async fn serve() {
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

    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)), 8080);

    println!("starting up the server on {}", addr.to_string());

    warp::serve(routes).run(addr).await
}
