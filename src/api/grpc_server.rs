use std::{
    error::Error as StdError,
    net::SocketAddr,
    sync::{Arc, atomic::AtomicBool},
};
use tokio::sync::oneshot;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::Server as TonicServer;
use vector_lib::tap::topology::WatchRx;

use super::grpc::ObservabilityService;
use crate::{config::Config, proto::observability::Server as ObservabilityServer};

/// gRPC API server for Vector observability.
pub struct GrpcServer {
    _shutdown: oneshot::Sender<()>,
    addr: SocketAddr,
}

impl GrpcServer {
    /// Start the gRPC API server.
    ///
    /// This creates a new gRPC server listening on the configured address and spawns
    /// it in the background. The server will shut down gracefully when this struct
    /// is dropped.
    ///
    /// Returns an error if the server fails to bind to the configured address.
    pub async fn start(
        config: &Config,
        watch_rx: WatchRx,
        running: Arc<AtomicBool>,
    ) -> crate::Result<Self> {
        let addr = config.api.address.ok_or_else(|| {
            crate::Error::from("API address not configured in config.api.address")
        })?;

        // Bind the TCP listener first to ensure the port is available
        // This will fail fast if the address is already in use
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            crate::Error::from(format!("Failed to bind gRPC API server to {}: {}", addr, e))
        })?;

        let actual_addr = listener
            .local_addr()
            .map_err(|e| crate::Error::from(format!("Failed to get local address: {}", e)))?;

        info!("GRPC API server bound to {}.", actual_addr);

        let service = ObservabilityService::new(watch_rx, running);

        let (_shutdown, rx) = oneshot::channel();

        // Spawn the server with the already-bound listener
        tokio::spawn(async move {
            let incoming = TcpListenerStream::new(listener);

            // Build reflection service for tools like grpcurl
            let reflection_service = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::proto::observability::FILE_DESCRIPTOR_SET,
                )
                .build()
                .expect("Failed to build reflection service");

            let result = TonicServer::builder()
                .add_service(ObservabilityServer::new(service))
                .add_service(reflection_service)
                .serve_with_incoming_shutdown(incoming, async {
                    rx.await.ok();
                    info!("GRPC API server shutting down.");
                })
                .await;

            if let Err(e) = result {
                error!(
                    message = "GRPC server encountered an error.",
                    error = %e,
                    error_source = ?e.source(),
                    bind_addr = %actual_addr,
                );
            }
        });

        info!("GRPC API server started on {}.", actual_addr);

        Ok(Self {
            _shutdown,
            addr: actual_addr,
        })
    }

    /// Get the address the server is listening on
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }
}
