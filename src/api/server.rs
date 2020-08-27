use super::{handler, schema};
use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig},
    QueryBuilder,
};
use async_graphql_warp::{graphql_subscription, GQLResponse};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use tokio::sync::oneshot;
use warp::{http::Response, Filter};

pub struct Server {
    /// Address for the API server to bind on
    address: SocketAddr,

    /// Transmission channel to trigger closure of a running API server
    trigger_cancel: oneshot::Sender<()>,

    /// Receiver signal to cancel running API server
    cancel_signal: Option<oneshot::Receiver<()>>,

    /// Enables the playground
    playground: bool,
}

impl Server {
    /// Returns a new API Server
    pub fn new(address: SocketAddr) -> Self {
        let (trigger_cancel, cancel_signal) = oneshot::channel::<()>();

        Server {
            address,
            trigger_cancel,
            cancel_signal: Some(cancel_signal),
            playground: true,
        }
    }

    pub fn set_playground(mut self, enable: bool) -> Self {
        self.playground = enable;
        self
    }

    /// String representation of the bound IP address
    pub fn ip(&self) -> IpAddr {
        self.address.ip()
    }

    /// String representation of the bound port
    pub fn port(&self) -> u16 {
        self.address.port()
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

        // Build the GraphQL schema
        let schema = schema::build_schema().finish();

        // Routes...

        // Health route
        let health_route = warp::path("health").and_then(handler::health);

        // GraphQL POST handler
        let graphql_post = async_graphql_warp::graphql(schema.clone()).and_then(
            |(schema, builder): (_, QueryBuilder)| async move {
                let resp = builder.execute(&schema).await;
                Ok::<_, Infallible>(GQLResponse::from(resp))
            },
        );

        // GraphQL playground
        let enable_playground = self.playground;
        let graphql_playground = warp::path("playground").map(move || match enable_playground {
            true => Response::builder()
                .header("content-type", "text/html")
                .body(playground_source(
                    GraphQLPlaygroundConfig::new("/graphql").subscription_endpoint("/graphql"),
                )),
            false => Response::builder()
                .header("content-type", "text/plain")
                .body(String::from("GraphQL playground has been disabled")),
        });

        let routes = balanced_or_tree!(
            graphql_subscription(schema),
            health_route,
            graphql_post,
            graphql_playground
        )
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

        let (_, server) =
            warp::serve(routes).bind_with_graceful_shutdown(self.address, async move {
                let _ = rx.await;
            });

        tokio::spawn(server);

        self
    }
}
