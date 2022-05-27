use crate::{
    internal_events::TcpBytesReceived,
    shutdown::{ShutdownSignal, ShutdownSignalToken},
    sources::util::AfterReadExt as _,
    tls::MaybeTlsSettings,
};
use futures::{FutureExt, StreamExt};
use http::{Request, Response};
use hyper::Body;
use std::{convert::Infallible, net::SocketAddr};
use tonic::{
    body::BoxBody,
    transport::server::{Connected, NamedService, Server},
};
use tower::Service;
use tracing::{Instrument, Span};

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
    let stream = listener.accept_stream().map(|result| {
        result.map(|socket| {
            let peer_addr = socket.connect_info().remote_addr;
            // TODO: Primary downside to this approach is that it's counting the raw bytes on the wire, which
            // will almost certainly be a much smaller number than the actual number of bytes when
            // accounting for compression.
            //
            // Possible solutions:
            // - write our own codec that works with `tonic::codec::Codec`, and write our own
            //   `prost_build::ServiceGenerator` that codegens using it, so that we can capture the size of the body
            //   after `tonic` decompresses it
            // - fork `tonic-build` to support overriding `Service::CODEC_PATH` in the builder config (see
            //   https://github.com/bmwill/tonic/commit/c409121811844494728a9ea8345ed2189855c870 for an example of doing
            //   this) and try and upstream it; we would still need to write the aforementioned codec, though
            // - switch to using the compression layer from `tower-http` to handle compression _before_ it hits `tonic`,
            //   which would then let us insert a layer right after it that would have access to the raw body before it
            //   gets decoded
            //
            // Number #3 is _probably_ easiest because it's "just" normal Tower middleware/layers, and there's already
            // the layer for handling compression, and it would give us more flexibility around only tracking the bytes
            // of specific service methods -- i.e. `push_events` -- rather than tracking all bytes that flow over the
            // gRPC connection, like health checks or any future enhancements that we/tonic adds.
            socket.after_read(move |byte_size| {
                emit!(TcpBytesReceived {
                    byte_size,
                    peer_addr,
                })
            })
        })
    });

    Server::builder()
        .trace_fn(move |_| span.clone())
        .add_service(service)
        .serve_with_incoming_shutdown(stream, shutdown.map(|token| tx.send(token).unwrap()))
        .in_current_span()
        .await?;

    drop(rx.await);

    Ok(())
}
