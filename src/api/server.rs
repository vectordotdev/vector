use super::{handler, schema, ShutdownTx};
use crate::{config, topology};
use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    Data, Request, Schema,
};
use async_graphql_warp::{graphql_subscription_with_data, Response as GQLResponse};
use std::{convert::Infallible, net::SocketAddr};
use tokio::sync::oneshot;
use warp::{filters::BoxedFilter, http::Response, Filter, Reply};

pub struct Server {
    _shutdown: ShutdownTx,
    addr: SocketAddr,
}

impl Server {
    /// Start the API server. This creates the routes and spawns a Warp server. The server is
    /// gracefully shut down when Self falls out of scope by way of the oneshot sender closing.
    pub fn start(config: &config::Config, watch_rx: topology::WatchRx) -> Self {
        let routes = make_routes(config.api.playground, watch_rx);

        let (_shutdown, rx) = oneshot::channel();
        let (addr, server) = warp::serve(routes).bind_with_graceful_shutdown(
            config.api.address.expect("No socket address"),
            async {
                rx.await.ok();
            },
        );

        // Update component schema with the config before starting the server.
        schema::components::update_config(config);

        // Spawn the server in the background.
        tokio::spawn(server);

        Self { _shutdown, addr }
    }

    /// Returns a copy of the SocketAddr that the server was started on.
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Update the configuration of a running server. While this instance method doesn't
    /// directly involve `self`, it provides a neater API to expose an internal implementation
    /// detail than exposing the function of the sub-mod directly.
    pub fn update_config(&self, config: &config::Config) {
        schema::components::update_config(config)
    }
}

fn make_routes(playground: bool, watch_tx: topology::WatchRx) -> BoxedFilter<(impl Reply,)> {
    // Build the GraphQL schema.
    let schema = schema::build_schema().finish();

    // Routes...

    // Health.
    let health = warp::path("health").and_then(handler::health);

    // 404.
    let not_found = warp::any().and_then(|| async { Err(warp::reject::not_found()) });

    // GraphQL query and subscription handler.
    let graphql_handler = warp::path("graphql").and(
        graphql_subscription_with_data(schema.clone(), move |_| async {
            let mut data = Data::default();
            data.insert(watch_tx);
            Ok(data)
        })
        .or(async_graphql_warp::graphql(schema).and_then(
            |(schema, request): (Schema<_, _, _>, Request)| async move {
                Ok::<_, Infallible>(GQLResponse::from(schema.execute(request).await))
            },
        )),
    );

    // GraphQL playground
    let graphql_playground = if playground {
        warp::path("playground")
            .map(move || {
                Response::builder()
                    .header("content-type", "text/html")
                    .body(playground_source(
                        GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/graphql"),
                    ))
            })
            .boxed()
    } else {
        not_found.boxed()
    };

    health
        .or(graphql_handler)
        .or(graphql_playground)
        .or(not_found)
        .with(
            warp::cors()
                .allow_any_origin()
                .allow_headers(vec![
                    "User-Agent",
                    "Sec-Fetch-Mode",
                    "Referer",
                    "Origin",
                    "Access-Control-Request-Method",
                    "Access-Control-Allow-Origin",
                    "Access-Control-Request-Headers",
                    "Content-Type",
                    "X-Apollo-Tracing", // for Apollo GraphQL clients
                    "Pragma",
                    "Host",
                    "Connection",
                    "Cache-Control",
                ])
                .allow_methods(vec!["POST", "GET"]),
        )
        .boxed()
}
