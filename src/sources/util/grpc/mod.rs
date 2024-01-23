use crate::{
    internal_events::{GrpcServerRequestReceived, GrpcServerResponseSent},
    shutdown::{ShutdownSignal, ShutdownSignalToken},
    tls::MaybeTlsSettings,
};
use futures::FutureExt;
use http::{Request, Response};
use hyper::Body;
use std::{convert::Infallible, net::SocketAddr, time::Duration};
use tonic::{
    body::BoxBody,
    transport::server::{NamedService, Server},
};
use tower::Service;
use tower_http::{
    classify::{GrpcErrorsAsFailures, SharedClassifier},
    trace::TraceLayer,
};
use tracing::Span;

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
        .layer(build_grpc_trace_layer(span.clone()))
        // This layer explicitly decompresses payloads, if compressed, and reports the number of message bytes we've
        // received if the message is processed successfully, aka `BytesReceived`. We do this because otherwise the only
        // access we have is either the event-specific bytes (the in-memory representation) or the raw bytes over the
        // wire prior to decompression... and if that case, any bytes at all, not just the ones we successfully process.
        //
        // The weaving of `tonic`, `axum`, `tower`, and `hyper` is fairly complex and there currently exists no way to
        // use independent `tower` layers when the request body itself (the body type, not the actual bytes) must be
        // modified or wrapped.. so instead of a cleaner design, we're opting here to bake it all together until the
        // crates are sufficiently flexible for us to craft a better design.
        .layer(DecompressionAndMetricsLayer)
        .add_service(service)
        .serve_with_incoming_shutdown(stream, shutdown.map(|token| tx.send(token).unwrap()))
        .await?;

    drop(rx.await);

    Ok(())
}

/// Builds a [TraceLayer] configured for a gRPC server.
///
/// This layer emits gPRC specific telemetry for messages received/sent and handler duration.
pub fn build_grpc_trace_layer(
    span: Span,
) -> TraceLayer<
    SharedClassifier<GrpcErrorsAsFailures>,
    impl Fn(&Request<Body>) -> Span + Clone,
    impl Fn(&Request<Body>, &Span) + Clone,
    impl Fn(&Response<BoxBody>, Duration, &Span) + Clone,
    (),
    (),
    (),
> {
    TraceLayer::new_for_grpc()
        .make_span_with(move |request: &Request<Body>| {
            // The path is defined as “/” {service name} “/” {method name}.
            let mut path = request.uri().path().split('/');
            let service = path.nth(1).unwrap_or("_unknown");
            let method = path.next().unwrap_or("_unknown");

            // This is an error span so that the labels are always present for metrics.
            error_span!(
               parent: &span,
               "grpc-request",
               grpc_service = service,
               grpc_method = method,
            )
        })
        .on_request(Box::new(|_request: &Request<Body>, _span: &Span| {
            emit!(GrpcServerRequestReceived);
        }))
        .on_response(
            |response: &Response<BoxBody>, latency: Duration, _span: &Span| {
                emit!(GrpcServerResponseSent { response, latency });
            },
        )
        .on_failure(())
        .on_body_chunk(())
        .on_eos(())
}
