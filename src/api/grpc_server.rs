use std::{error::Error as StdError, net::SocketAddr};
use tokio::sync::oneshot;
use tonic::transport::Server as TonicServer;
use vector_lib::tap::topology::WatchRx;

use super::grpc::ObservabilityService;
use crate::{config::Config, proto::observability::Server as ObservabilityServer};

/// gRPC API server for Vector observability.
///
/// This server provides real-time metrics and component information via gRPC,
/// replacing the GraphQL API with a more efficient binary protocol.
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
    pub async fn start(config: &Config, watch_rx: WatchRx) -> crate::Result<Self> {
        let addr = config.api.address.ok_or_else(|| {
            crate::Error::from("API address not configured in config.api.address")
        })?;

        let service = ObservabilityService::new(watch_rx);

        let (_shutdown, rx) = oneshot::channel();

        // Clone address for the spawned task
        let bind_addr = addr;

        tokio::spawn(async move {
            info!("Starting gRPC API server on {}.", bind_addr);

            let result = TonicServer::builder()
                .add_service(ObservabilityServer::new(service))
                .serve_with_shutdown(bind_addr, async {
                    rx.await.ok();
                    info!("GRPC API server shutting down.");
                })
                .await;

            if let Err(e) = result {
                error!(
                    message = "GRPC server encountered an error.",
                    error = %e,
                    error_source = ?e.source(),
                    bind_addr = %bind_addr,
                );
            }
        });

        info!("GRPC API server started on {}.", addr);

        Ok(Self { _shutdown, addr })
    }

    /// Get the address the server is listening on
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }
}
