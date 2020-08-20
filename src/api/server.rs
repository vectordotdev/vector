use super::handler;

use crate::api::schema::{schema, Context};
use crate::tls::{MaybeTlsSettings, TlsConfig};
use futures::{channel::oneshot, FutureExt as _};
use juniper_graphql_ws::ConnectionConfig;
use juniper_warp::playground_filter;
use juniper_warp::subscriptions::serve_graphql_ws;
use std::net::SocketAddr;
use std::sync::Arc;
use warp::Filter;

pub struct Server {
    address: SocketAddr,
    tls: Option<TlsConfig>,
}

type CancelSignal = oneshot::Receiver<()>;

impl Server {
    /// Returns a new API Server
    pub fn new(address: SocketAddr) -> Server {
        Server { address, tls: None }
    }

    /// String representation of the bound IP address
    pub fn ip(&self) -> String {
        self.address.ip().to_string()
    }

    /// String representation of the bound port
    pub fn port(&self) -> String {
        self.address.port().to_string()
    }

    /// Run the API server
    pub async fn run(self, cancel: CancelSignal) {
        // GraphQL state
        let qm_schema = schema();
        let qm_state = warp::any().map(move || Context::new());
        let root_node = Arc::new(schema());

        let health_route = warp::path("health").and_then(handler::health);

        let graphql_route = warp::path("graphql")
            .and(warp::ws())
            .map(move |ws: warp::ws::Ws| {
                let root_node = Arc::clone(&root_node);
                // let tx = Arc::clone(&tx);
                ws.on_upgrade(move |websocket| async move {
                    serve_graphql_ws(websocket, root_node, ConnectionConfig::new(Context::new()))
                        .map(|r| {
                            if let Err(e) = r {
                                println!("Websocket error: {}", e);
                            }
                        })
                        .await
                })
            })
            .map(|reply| {
                // TODO#584: remove this workaround
                warp::reply::with_header(reply, "Sec-WebSocket-Protocol", "graphql-ws")
            })
            .or(warp::post()
                .and(warp::path("graphql"))
                .and(juniper_warp::make_graphql_filter(
                    qm_schema,
                    qm_state.boxed().into(),
                )))
            .or(warp::get()
                .and(warp::path("playground"))
                .and(playground_filter("/graphql", Some("/graphql"))));

        let routes = health_route.or(graphql_route).with(
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
                    "X-Apollo-Tracing", // for Apollo clients
                    "Pragma",
                    "Host",
                    "Connection",
                    "Cache-Control",
                ])
                .allow_methods(vec!["POST", "GET"]),
        );

        let tls = MaybeTlsSettings::from_config(&self.tls, true).unwrap();
        let mut listener = tls.bind(&self.address).await.unwrap();

        let _ = warp::serve(routes)
            .serve_incoming_with_graceful_shutdown(listener.incoming(), async move {
                cancel.await.unwrap();
            })
            .await;
    }
}
