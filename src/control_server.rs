use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{rejection::JsonRejection, Json},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use futures::FutureExt;
use http::StatusCode;
use hyper::server::{self, conn::Http};
use serde_json::json;
use stream_cancel::Tripwire;
use tokio::{net::UnixListener, sync::Mutex};
use tokio_stream::wrappers::UnixListenerStream;

use crate::{
    config::ConfigBuilder,
    topology::{ReloadOutcome, TopologyController},
};

pub struct ControlServer {
    listener: UnixListener,
    topology_controller: Arc<Mutex<TopologyController>>,
    shutdown_signal: Tripwire,
    socket_path: PathBuf,
}

impl ControlServer {
    pub fn bind(
        socket_path: impl AsRef<Path>,
        topology_controller: Arc<Mutex<TopologyController>>,
        shutdown_signal: Tripwire,
    ) -> Result<Self, crate::Error> {
        // Try to remove any lingering socket from previous runs
        if socket_path.as_ref().try_exists()? {
            std::fs::remove_file(&socket_path)?;
        }
        let listener = UnixListener::bind(&socket_path)?;

        Ok(ControlServer {
            listener,
            topology_controller,
            shutdown_signal,
            socket_path: socket_path.as_ref().to_path_buf(),
        })
    }

    pub async fn run(self) -> Result<(), crate::Error> {
        let app = Router::new()
            .route("/ping", get(|| async { "pong" }))
            .route(
                "/config",
                post(
                    |payload: Result<Json<ConfigBuilder>, JsonRejection>| async move {
                        let Ok(mut controller) = self.topology_controller.try_lock() else {
                            return Err(ApiError::Locked)
                        };

                        let Json(builder) = payload.map_err(ApiError::Json)?;
                        let new_config = builder.build().map_err(ApiError::Build)?;
                        match controller.reload(Some(new_config)).await {
                            ReloadOutcome::Success => Ok(StatusCode::CREATED),
                            ReloadOutcome::MissingApiKey => Err(ApiError::MissingApiKey),
                            // TODO: return these errors up from inner topology methods
                            ReloadOutcome::RolledBack => Err(ApiError::RolledBack(vec![])),
                            ReloadOutcome::FatalError => Err(ApiError::Fatal(vec![])),
                            ReloadOutcome::NoConfig => {
                                unreachable!("Some(config) was passed above")
                            }
                        }
                    },
                ),
            );

        let accept = server::accept::from_stream(UnixListenerStream::new(self.listener));
        let server = server::Builder::new(accept, Http::new()).serve(app.into_make_service());
        let graceful = server.with_graceful_shutdown(self.shutdown_signal.map(|_| ()));

        info!(message = "Starting Vector control server.", socket_path = ?self.socket_path);

        graceful.await?;

        // Try to clean up after ourselves
        std::fs::remove_file(&self.socket_path).ok();

        Ok(())
    }
}

enum ApiError {
    Locked,
    Json(JsonRejection),
    Build(Vec<String>),
    MissingApiKey,
    RolledBack(Vec<String>),
    Fatal(Vec<String>),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Locked => (
                StatusCode::CONFLICT,
                Json(json!({
                    "reason": "topology currently locked",
                })),
            ),
            ApiError::Json(rejection) => (
                rejection.status(),
                Json(json!({
                    "reason": rejection.body_text(),
                })),
            ),
            ApiError::Build(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "reason": "config build error",
                    "errors": errors,
                })),
            ),
            ApiError::MissingApiKey => (
                StatusCode::UNAUTHORIZED,
                Json(json!({
                    "reason": "missing API key",
                })),
            ),
            ApiError::RolledBack(errors) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(json!({
                    "reason": "reload error, rolled back to previous config",
                    "errors": errors,
                })),
            ),
            ApiError::Fatal(errors) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "reason": "fatal reload error, failed to roll back to previous config",
                    "errors": errors,
                })),
            ),
        }
        .into_response()
    }
}
