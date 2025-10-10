use std::time::Duration;

use k8s_openapi::api::core::v1::Pod;
use kube::runtime::reflector::Store;
use tokio::sync::mpsc;
use tracing::{info, warn};

use super::{lifecycle::ShutdownHandle, pod_info::PodInfo};

/// Pod information publisher that monitors Kubernetes pod state changes
/// and publishes pod information through a channel.
///
/// This follows the Docker logs EventStreamBuilder pattern where spawned tasks
/// communicate with the main task via channels.
pub struct PodPublisher {
    pod_state: Store<Pod>,
    sender: mpsc::UnboundedSender<PodInfo>,
    shutdown: ShutdownHandle,
}
impl PodPublisher {
    /// Create a new pod publisher
    pub fn new(
        pod_state: Store<Pod>,
        sender: mpsc::UnboundedSender<PodInfo>,
        shutdown: ShutdownHandle,
    ) -> Self {
        Self {
            pod_state,
            sender,
            shutdown,
        }
    }

    /// Start the pod monitoring task that publishes pod information periodically
    ///
    /// This task:
    /// - Monitors pod_state every 5 seconds
    /// - Extracts pod names from the current pod state
    /// - Sends pod names through the channel to the main task
    /// - Handles shutdown signals gracefully
    pub async fn run(mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(5));

        info!("Pod monitoring task started");

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // Get all current pods and publish their detailed information
                    let pods = self.pod_state.state();
                    for pod in pods.iter() {
                        if let Ok(pod_info) = PodInfo::try_from(pod.as_ref()) {
                            // Only publish running pods for log fetching
                            if pod_info.is_running() {
                                if let Err(_) = self.sender.send(pod_info) {
                                    warn!("Failed to send pod info through channel");
                                    return;
                                }
                            }
                        }
                    }
                }
                _ = &mut self.shutdown => {
                    info!("Pod monitoring task shutting down");
                    return;
                }
            }
        }
    }
}
