use super::handler;

use crate::{
    shutdown::ShutdownSignal,
    tls::{MaybeTlsSettings, TlsConfig},
};
use futures::{compat::Future01CompatExt, FutureExt, TryFutureExt};
use std::net::SocketAddr;
use warp::Filter;

pub struct Server {
    address: SocketAddr,
    tls: Option<TlsConfig>,
}

impl Server {
    pub fn new(address: SocketAddr) -> Server {
        Server { address, tls: None }
    }

    pub async fn build(&self, shutdown: ShutdownSignal) -> crate::Result<crate::sources::Source> {
        let health_route = warp::path("health").and_then(handler::health);

        let services = health_route.with(
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
                    "X-Apollo-Tracing", // for Apollo platform clients
                    "Pragma",
                    "Host",
                    "Connection",
                    "Cache-Control",
                ])
                .allow_methods(vec!["POST", "GET"]),
        );

        let tls = MaybeTlsSettings::from_config(&self.tls, true)?;
        let mut listener = tls.bind(&self.address).await?;

        let fut = async move {
            let _ = warp::serve(services)
                .serve_incoming_with_graceful_shutdown(
                    listener.incoming(),
                    shutdown.clone().compat().map(|_| ()),
                )
                .await;
            // We need to drop the last copy of ShutdownSignalToken only after server has shut down.
            drop(shutdown);
            Ok(())
        };

        Ok(Box::new(fut.boxed().compat()))
    }
}
