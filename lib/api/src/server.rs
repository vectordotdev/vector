use super::handler;

use crate::schema::{schema, Context};
use juniper::futures::FutureExt;
use juniper_graphql_ws::ConnectionConfig;
use juniper_warp::playground_filter;
use juniper_warp::subscriptions::serve_graphql_ws;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio;
use tokio::sync::oneshot;
use tokio::sync::oneshot::{Receiver, Sender};
use warp::filters::BoxedFilter;
use warp::{Filter, Reply};

pub struct Server {
    address: SocketAddr,
    trigger_cancel: Sender<()>,
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

fn make_routes() -> BoxedFilter<(impl Reply,)> {
    // health
    let health_route = warp::path("health").and_then(handler::health);

    let qm_schema = schema();
    let qm_state = warp::any().map(move || Context::new());

    let root_node = Arc::new(schema());

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
                qm_state.boxed(),
            )))
        .or(warp::get()
            .and(warp::path("playground"))
            .and(playground_filter("/graphql", Some("/graphql"))));

    // all routes - allow any origin
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
