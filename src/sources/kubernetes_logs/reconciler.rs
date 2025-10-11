use super::pod_info::PodInfo;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::reflector::Store;
use kube::{Api, Client, api::LogParams};
use tracing::{error, info, warn};

pub struct Reconciler {
    pod_state: Store<Pod>,
    client: Client,
}

impl Reconciler {
    pub fn new(pod_state: Store<Pod>, client: Client) -> Self {
        Self { pod_state, client }
    }

    pub async fn reconcile(&self) {
        // TODO: replace timer with watcher for pod state changes and reconcile accordingly
        let mut timer = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = timer.tick() => {
                    self.perform_reconciliation().await;
                }
            }
        }
    }

    async fn perform_reconciliation(&self) {
        // Placeholder for reconciliation logic
        info!("Performing reconciliation of pod states");
        // Actual reconciliation logic would go here
        self.pod_state
            .state()
            .iter()
            .map(|pod| PodInfo::from(pod.as_ref()))
            .for_each(|pod_info| {
                info!("PodInfo: {:?}", pod_info);
                // self.fetch_pod_logs(&pod_info).await;
                let tailer = LogTailer::new(self.client.clone());
                let _status = tailer.start(&pod_info);
                // TODO: Store tailer status in a thread-safe way
            });
    }
}

struct LogTailer {
    client: Client,
}

enum TailStatus {
    Running,
    // Stopped,
}

impl LogTailer {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn start(&self, pod_info: &PodInfo) -> TailStatus {
        let pod_info = pod_info.clone();
        let client = self.client.clone();
        tokio::spawn(async move {
            let tailer = LogTailer { client };
            if let Err(e) = tailer.tail_log(&pod_info).await {
                error!("Error tailing logs for pod '{}': {}", pod_info.name, e);
            }
        });
        TailStatus::Running
    }

    pub async fn tail_log(
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
