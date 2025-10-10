use std::collections::HashSet;

use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client, api::LogParams};
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use super::pod_info::PodInfo;
use crate::shutdown::ShutdownSignal;

/// Pod information subscriber that receives pod information from the publisher
/// and fetches logs from Kubernetes API.
///
/// This follows the Docker logs main future pattern where the main task
/// receives data from spawned tasks via channels.
pub struct PodSubscriber {
    receiver: mpsc::UnboundedReceiver<PodInfo>,
    shutdown: ShutdownSignal,
    client: Client,
    /// Track pods we've already started processing to avoid duplicates
    processed_pods: HashSet<String>,
}

impl PodSubscriber {
    /// Create a new pod subscriber
    pub fn new(
        receiver: mpsc::UnboundedReceiver<PodInfo>,
        shutdown: ShutdownSignal,
        client: Client,
    ) -> Self {
        Self {
            receiver,
            shutdown,
            client,
            processed_pods: HashSet::new(),
        }
    }

    /// Start the pod consumer task that receives and processes pod information
    ///
    /// This task:
    /// - Receives pod information from the channel
    /// - Fetches logs from Kubernetes API for each pod
    /// - Handles channel closure and shutdown gracefully
    pub async fn run(mut self) {
        info!("Pod consumer task started");

        loop {
            tokio::select! {
                pod_info = self.receiver.recv() => {
                    match pod_info {
                        Some(pod_info) => {
                            // Check if we've already processed this pod to avoid duplicates
                            if !self.processed_pods.contains(&pod_info.uid) {
                                self.processed_pods.insert(pod_info.uid.clone());
                                info!("Processing new pod: {} in namespace {}", pod_info.name, pod_info.namespace);

                                // Fetch logs for this pod
                                if let Err(e) = self.fetch_pod_logs(&pod_info).await {
                                    error!("Failed to fetch logs for pod {}: {}", pod_info.name, e);
                                }
                            }
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

    /// Fetch logs from Kubernetes API for a specific pod
    async fn fetch_pod_logs(
        &self,
        pod_info: &PodInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &pod_info.namespace);

        // For each container in the pod, fetch its logs
        for container in &pod_info.containers {
            info!(
                "Fetching logs for container '{}' in pod '{}'",
                container, pod_info.name
            );

            let log_params = LogParams {
                container: Some(container.clone()),
                follow: false,        // For now, just get recent logs, not streaming
                tail_lines: Some(10), // Get last 10 lines
                timestamps: true,
                ..Default::default()
            };

            match pods.logs(&pod_info.name, &log_params).await {
                Ok(logs) => {
                    // Process the logs - for now just print them
                    // In a full implementation, these would be sent to the Vector event pipeline
                    if !logs.is_empty() {
                        info!(
                            "=== Logs from pod '{}', container '{}' ===",
                            pod_info.name, container
                        );
                        for line in logs.lines().take(5) {
                            // Limit output for demo
                            info!("LOG: {}", line);
                        }
                        info!("=== End of logs ===");
                    } else {
                        info!(
                            "No logs available for pod '{}', container '{}'",
                            pod_info.name, container
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to fetch logs for pod '{}', container '{}': {}",
                        pod_info.name, container, e
                    );
                }
            }
        }

        Ok(())
    }
}
