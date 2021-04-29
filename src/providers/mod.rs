pub mod http;

use tokio::sync::oneshot;

/// Shutdown trigger types for a provider.
type ShutdownRx = oneshot::Receiver<()>;
pub type ShutdownTx = oneshot::Sender<()>;

/// A provider returns an initial configuration string, if successful.
pub type Result = std::result::Result<String, &'static str>;

/// Create a shutdown trigger for a provider.
fn shutdown_trigger() -> (ShutdownTx, ShutdownRx) {
    oneshot::channel()
}
