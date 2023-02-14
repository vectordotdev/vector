use crate::{
    shutdown::{ShutdownSignal, ShutdownSignalToken},
    tls::MaybeTlsSettings,
};
use futures::FutureExt;
use http::{Request, Response};
use hyper::Body;
use std::{convert::Infallible, net::SocketAddr};
use tonic::{
    body::BoxBody,
    transport::server::{NamedService, Server},
};
use tower::Service;
use tracing::{Instrument, Span};

mod decompression;
pub use self::decompression::{DecompressionAndMetrics, DecompressionAndMetricsLayer};

pub async fn run_grpc_server<S>(
    address: SocketAddr,
    tls_settings: MaybeTlsSettings,
    service: S,
    shutdown: ShutdownSignal,
) -> crate::Result<()>
where
    S: Service<Request<Body>, Response = Response<BoxBody>, Error = Infallible>
        + NamedService
        + Clone
        + Send
        + 'static,
    S::Future: Send + 'static,
{
    let span = Span::current();
    let (tx, rx) = tokio::sync::oneshot::channel::<ShutdownSignalToken>();
    let listener = tls_settings.bind(&address).await?;
    let stream = listener.accept_stream();

    info!(%address, "Building gRPC server.");

    Server::builder()
        .trace_fn(move |_| span.clone())
        // This layer explicitly decompresses payloads, if compressed, and reports the number of message bytes we've
        // received if the message is processed successfully, aka `BytesReceived`. We do this because otherwise the only
        // access we have is either the event-specific bytes (the in-memory representation) or the raw bytes over the
        // wire prior to decompression... and if that case, any bytes at all, not just the ones we successfully process.
        //
        // The weaving of `tonic`, `axum`, `tower`, and `hyper` is fairly complex and there currently exists no way to
        // use independent `tower` layers when the request body itself (the body type, not the actual bytes) must be
        // modified or wrapped.. so instead of a cleaner design, we're opting here to bake it all together until the
        // crates are sufficiently flexible for us to craft a better design.
        .layer(DecompressionAndMetricsLayer::default())
        .add_service(service)
        .serve_with_incoming_shutdown(stream, shutdown.map(|token| tx.send(token).unwrap()))
        .in_current_span()
        .await?;

    drop(rx.await);

    Ok(())
}
