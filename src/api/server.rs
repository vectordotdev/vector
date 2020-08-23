use super::{handler, schema::build_schema};

use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    QueryBuilder,
};
use async_graphql_warp::{graphql_subscription, GQLResponse};
use std::convert::Infallible;
use std::net::SocketAddr;
use tokio;
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};
use warp::filters::BoxedFilter;
use warp::{http::Response, Filter, Reply};

pub struct Server {
    /// Address for the API server to bind on
    address: SocketAddr,

    /// Transmission channel to trigger closure of a running API server
    trigger_cancel: Sender<()>,

    /// Receiver signal to cancel running API server
    cancel_signal: Option<Receiver<()>>,
}

impl Server {
    /// Returns a new API Server
    pub fn new(address: SocketAddr) -> Server {
        let (trigger_cancel, cancel_signal) = oneshot::channel::<()>();

        Server {
            address,
            trigger_cancel,
            cancel_signal: Some(cancel_signal),
        }
    }

    /// String representation of the bound IP address
    pub fn ip(&self) -> String {
        self.address.ip().to_string()
    }

    /// String representation of the bound port
    pub fn port(&self) -> String {
        self.address.port().to_string()
    }

    /// Stops the running API server
    pub fn stop(self) {
        let _ = self.trigger_cancel.send(());
    }

    /// Run the API server
    pub async fn run(mut self) -> Self {
        let rx = self
            .cancel_signal
            .take()
            .expect("Run can only be called once");

        let (_, server) =
            warp::serve(make_routes()).bind_with_graceful_shutdown(self.address, async move {
                let _ = rx.await;
            });

        tokio::spawn(server);

        self
    }
}

/// Builds Warp routes, to be served
fn make_routes() -> BoxedFilter<(impl Reply,)> {
    // Health route
    let health_route = warp::path("health").and_then(handler::health);

    // Build the GraphQL schema
    let schema = build_schema().finish();

    // GraphQL POST handler
    let graphql_post = async_graphql_warp::graphql(schema.clone()).and_then(
        |(schema, builder): (_, QueryBuilder)| async move {
            let resp = builder.execute(&schema).await;
            Ok::<_, Infallible>(GQLResponse::from(resp))
        },
    );

    // GraphQL playground. Also sets up /graphql as an endpoint for both queries + subscriptions
    let graphql_playground = warp::path("playground").map(|| {
        Response::builder()
            .header("content-type", "text/html")
            .body(playground_source(
                GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/graphql"),
            ))
    });

    // All routes - allow any origin
    let routes = graphql_subscription(schema)
        .or(graphql_post)
        .or(graphql_playground)
        .or(health_route)
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
        );

    routes.boxed()
}
