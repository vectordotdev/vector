use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::Json,
    routing::{get, post},
    Router,
};
use futures::FutureExt;
use hyper::server::{self, conn::Http};
use stream_cancel::Tripwire;
use tokio::{net::UnixListener, sync::Mutex};
use tokio_stream::wrappers::UnixListenerStream;

use crate::{config::ConfigBuilder, topology::TopologyController};

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
                post(|Json(builder): Json<ConfigBuilder>| async move {
                    if let Ok(mut controller) = self.topology_controller.try_lock() {
                        let new_config = builder.build().ok();
                        controller.reload(new_config).await;
                        Ok(())
                    } else {
                        Err("not ready")
                    }
                }),
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
