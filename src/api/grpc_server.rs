use std::{
    error::Error as StdError,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use tokio::sync::oneshot;
use tonic::service::Routes;
use tonic_health::server::{HealthReporter, health_reporter};
use vector_lib::tap::topology::WatchRx;

use super::grpc::ObservabilityService;
use crate::{config::Config, proto::observability::Server as ObservabilityServer};

/// Shared flag backing the HTTP `/health` endpoint. Mirrors the gRPC
/// `HealthReporter` serving status so HTTP and gRPC probes agree.
type ServingState = Arc<AtomicBool>;

/// gRPC API server for Vector observability.
pub struct GrpcServer {
    _shutdown: oneshot::Sender<()>,
    health_reporter: HealthReporter,
    serving: ServingState,
    addr: SocketAddr,
}

impl GrpcServer {
    /// Start the gRPC API server.
    ///
    /// This creates a new gRPC server listening on the configured address and spawns
    /// it in the background. The server will shut down gracefully when this struct
    /// is dropped.
    ///
    /// # Errors
    ///
    /// Returns an error if `config.api.address` is not set, the server fails to bind to the
    /// configured address, or the bound listener's local address cannot be read.
    ///
    /// # Panics
    ///
    /// The spawned server task panics if the gRPC reflection service cannot be built from the
    /// embedded file descriptor sets.
    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::used_underscore_binding,
        reason = "Keep the existing binding names and resource lifetimes during the lint rollout."
    )]
    pub async fn start(config: &Config, watch_rx: WatchRx) -> crate::Result<Self> {
        let addr = config.api.address.ok_or_else(|| {
            crate::Error::from("API address not configured in config.api.address")
        })?;

        // Bind the TCP listener first to ensure the port is available
        // This will fail fast if the address is already in use
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|e| {
            crate::Error::from(format!("Failed to bind gRPC API server to {addr}: {e}"))
        })?;

        let actual_addr = listener
            .local_addr()
            .map_err(|e| crate::Error::from(format!("Failed to get local address: {e}")))?;

        info!("GRPC API server bound to {actual_addr}.");

        let service = ObservabilityService::new(watch_rx);

        // Create the standard gRPC health service (grpc.health.v1.Health).
        // The empty service ("") is registered as SERVING by default.
        let (health_reporter, health_service) = health_reporter();

        let serving: ServingState = Arc::new(AtomicBool::new(true));

        let (_shutdown, rx) = oneshot::channel();

        let router_serving = Arc::clone(&serving);

        // Spawn the server with the already-bound listener
        crate::spawn_in_current_span(async move {
            // Build the v1alpha reflection service for tools like grpcurl.
            let reflection_service = tonic_reflection::server::Builder::configure()
                .register_encoded_file_descriptor_set(
                    crate::proto::observability::FILE_DESCRIPTOR_SET,
                )
                .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
                .build_v1alpha()
                .expect("Failed to build reflection service");

            // Merge the gRPC services with the HTTP router so both protocols share
            // the same port.
            let router = Routes::new(health_service)
                .add_service(ObservabilityServer::new(service))
                .add_service(reflection_service)
                .into_axum_router()
                .merge(http_router(router_serving));

            let result = tonic::transport::Server::builder()
                .accept_http1(true)
                .http2_max_header_list_size(vector_common::constants::GRPC_MAX_HEADER_LIST_SIZE)
                .add_routes(router.into())
                .serve_with_incoming_shutdown(
                    tokio_stream::wrappers::TcpListenerStream::new(listener),
                    async {
                        rx.await.ok();
                        info!("GRPC API server shutting down.");
                    },
                )
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

        info!("GRPC API server started on {actual_addr}.");

        Ok(Self {
            _shutdown,
            health_reporter,
            serving,
            addr: actual_addr,
        })
    }

    /// Signal that the server is no longer serving.
    ///
    /// Call this **before** draining the topology so that Kubernetes gRPC
    /// readiness probes and HTTP `/health` probes fail early and the pod is
    /// removed from endpoints before the process exits.
    pub async fn set_not_serving(&mut self) {
        self.serving.store(false, Ordering::Relaxed);
        self.health_reporter
            .set_service_status("", tonic_health::ServingStatus::NotServing)
            .await;
    }

    /// Get the address the server is listening on
    #[must_use]
    pub const fn addr(&self) -> SocketAddr {
        self.addr
    }
}

/// Axum router exposing `GET`/`HEAD /health`.
///
/// Returns `200 {"ok":true}` while the server is serving and
/// `503 {"ok":false}` once [`GrpcServer::set_not_serving`] has been called.
/// Preserves the response shape expected by existing HTTP health probes such as
/// Kubernetes and load balancers.
fn http_router(state: ServingState) -> Router {
    Router::new()
        .route("/health", get(health_handler).head(health_handler))
        .with_state(state)
}

async fn health_handler(State(state): State<ServingState>) -> impl IntoResponse {
    if state.load(Ordering::Relaxed) {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"ok":true}"#,
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"ok":false}"#,
        )
    }
}
