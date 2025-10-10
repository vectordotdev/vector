use tokio::sync::mpsc;
use tracing::info;

use crate::shutdown::ShutdownSignal;

/// Pod information subscriber that receives pod information from the publisher
/// and processes it (currently just logging).
///
/// This follows the Docker logs main future pattern where the main task
/// receives data from spawned tasks via channels.
pub struct PodSubscriber {
    receiver: mpsc::UnboundedReceiver<String>,
    shutdown: ShutdownSignal,
}

impl PodSubscriber {
    /// Create a new pod subscriber
    pub fn new(receiver: mpsc::UnboundedReceiver<String>, shutdown: ShutdownSignal) -> Self {
        Self { receiver, shutdown }
    }

    /// Start the pod consumer task that receives and processes pod information
    ///
    /// This task:
    /// - Receives pod names from the channel
    /// - Processes pod information (currently just prints names)
    /// - Handles channel closure and shutdown gracefully
    ///
    /// In the future, this is where K8s logs API calls would be made
    /// instead of just printing the pod names.
    pub async fn run(mut self) {
        info!("Pod consumer task started");

        loop {
            tokio::select! {
                pod_name = self.receiver.recv() => {
                    match pod_name {
                        Some(name) => {
                            info!("Received pod name: {}", name);
                            // TODO: Here is where K8s logs API calls would be made
                            // instead of just logging the pod name
                        }
                        None => {
                            info!("Pod info channel closed");
                            break;
                        }
                    }
                }
                _ = self.shutdown.clone() => {
                    info!("Pod consumer task shutting down");
                    break;
                }
            }
        }
    }
}
