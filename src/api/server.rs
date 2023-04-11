use std::{
    convert::Infallible,
    net::SocketAddr,
    sync::{atomic::AtomicBool, Arc},
};

use async_graphql::{
    http::{playground_source, GraphQLPlaygroundConfig, WebSocketProtocols},
    Data, Request, Schema,
};
use async_graphql_warp::{graphql_protocol, GraphQLResponse, GraphQLWebSocket};
use tokio::sync::oneshot;
use warp::{filters::BoxedFilter, http::Response, ws::Ws, Filter, Reply};

use super::{handler, schema, ShutdownTx};
use crate::{
    config,
    internal_events::{SocketBindError, SocketMode},
    topology,
};

pub struct Server {
    _shutdown: ShutdownTx,
    addr: SocketAddr,
}

impl Server {
    /// Start the API server. This creates the routes and spawns a Warp server. The server is
    /// gracefully shut down when Self falls out of scope by way of the oneshot sender closing.
    pub fn start(
        config: &config::Config,
        watch_rx: topology::WatchRx,
        running: Arc<AtomicBool>,
        runtime: &tokio::runtime::Runtime,
    ) -> crate::Result<Self> {
        let routes = make_routes(config.api.playground, watch_rx, running);

        let (_shutdown, rx) = oneshot::channel();
        // warp uses `tokio::spawn` and so needs us to enter the runtime context.
        let _guard = runtime.enter();
        let (addr, server) = warp::serve(routes)
            .try_bind_with_graceful_shutdown(
                config.api.address.expect("No socket address"),
                async {
                    rx.await.ok();
                },
            )
            .map_err(|error| {
                emit!(SocketBindError {
                    mode: SocketMode::Tcp,
                    error: &error,
                });
                error
            })?;

        // Update component schema with the config before starting the server.
        schema::components::update_config(config);

        // Spawn the server in the background.
        runtime.spawn(server);

        Ok(Self { _shutdown, addr })
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

fn make_routes(
    playground: bool,
    watch_tx: topology::WatchRx,
    running: Arc<AtomicBool>,
) -> BoxedFilter<(impl Reply,)> {
    // Routes...

    // Health.
    let health = warp::path("health")
        .and(with_shared(running))
        .and_then(handler::health);

    // 404.
    let not_found = warp::any().and_then(|| async { Err(warp::reject::not_found()) });

    // GraphQL subscription handler. Creates a Warp WebSocket handler and for each connection,
    // parses the required headers for GraphQL and builds per-connection context based on the
    // provided `WatchTx` channel sender. This allows GraphQL resolvers to subscribe to
    // topology changes.
    let graphql_subscription_handler =
        warp::ws()
            .and(graphql_protocol())
            .map(move |ws: Ws, protocol: WebSocketProtocols| {
                let schema = schema::build_schema().finish();
                let watch_tx = watch_tx.clone();

                let reply = ws.on_upgrade(move |socket| {
                    let mut data = Data::default();
                    data.insert(watch_tx);

                    GraphQLWebSocket::new(socket, schema, protocol)
                        .with_data(data)
                        .serve()
                });

                warp::reply::with_header(
                    reply,
                    "Sec-WebSocket-Protocol",
                    protocol.sec_websocket_protocol(),
                )
            });

    // Handle GraphQL queries. Headers will first be parsed to determine whether the query is
    // a subscription and if so, an attempt will be made to upgrade the connection to WebSockets.
    // All other queries will fall back to the default HTTP handler.
    let graphql_handler = warp::path("graphql").and(graphql_subscription_handler.or(
        async_graphql_warp::graphql(schema::build_schema().finish()).and_then(
            |(schema, request): (Schema<_, _, _>, Request)| async move {
                Ok::<_, Infallible>(GraphQLResponse::from(schema.execute(request).await))
            },
        ),
    ));

    // Provide a playground for executing GraphQL queries/mutations/subscriptions.
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

    // Wire up the health + GraphQL endpoints. Provides a permissive CORS policy to allow for
    // cross-origin interaction with the Vector API.
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

fn with_shared(
    shared: Arc<AtomicBool>,
) -> impl Filter<Extract = (Arc<AtomicBool>,), Error = Infallible> + Clone {
    warp::any().map(move || Arc::<AtomicBool>::clone(&shared))
}
