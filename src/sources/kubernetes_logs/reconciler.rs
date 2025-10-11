use super::pod_info::PodInfo;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::reflector::Store;
use kube::{Api, Client, api::LogParams};
use tracing::{error, info, warn};

pub struct Reconciler {
    pod_state: Store<Pod>,
    tailer: LogTailer,
}

impl Reconciler {
    pub fn new(pod_state: Store<Pod>, client: Client) -> Self {
        let tailer = LogTailer::new(client.clone());
        Self { pod_state, tailer }
    }

    pub async fn run(&self) {
        // TODO: replace timer with watcher for pod state changes and reconcile accordingly
        let mut timer = tokio::time::interval(tokio::time::Duration::from_secs(10));
        loop {
            tokio::select! {
                _ = timer.tick() => {
                    // self.perform_reconciliation().await;
                }
            }
        }
    }

    pub async fn handle_running_pods(self) -> crate::Result<Self> {
        info!("Performing reconciliation of pod states");

        let pods: Vec<_> = self
            .pod_state
            .state()
            .iter()
            .map(|pod| PodInfo::from(pod.as_ref()))
            .collect();

        if pods.is_empty() {
            warn!("No pods found in pod store. The store might not be populated yet.");
            return Ok(self);
        }

        info!("Found {} pods in store", pods.len());

        // Filter for running pods and start tailing their logs
        let running_pods: Vec<_> = pods
            .into_iter()
            .filter(|pod_info| match &pod_info.phase {
                Some(phase) if phase == "Running" => {
                    info!(
                        "Pod '{}' is running with {} containers",
                        pod_info.name,
                        pod_info.containers.len()
                    );
                    true
                }
                Some(phase) => {
                    info!("Skipping pod '{}' in phase '{}'", pod_info.name, phase);
                    false
                }
                None => {
                    info!("Skipping pod '{}' with unknown phase", pod_info.name);
                    false
                }
            })
            .collect();

        if running_pods.is_empty() {
            info!("No running pods found to tail logs from");
        } else {
            info!(
                "Starting log tailing for {} running pods",
                running_pods.len()
            );
            for pod_info in running_pods {
                info!(
                    "Starting tailer for pod '{}' in namespace '{}'",
                    pod_info.name, pod_info.namespace
                );
                let _status = self.tailer.start(&pod_info);
                // TODO: Store tailer status in a thread-safe way
            }
        }

        Ok(self)
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

        info!(
            "Starting log tailing for pod '{}' in namespace '{}' with {} containers",
            pod_info.name,
            pod_info.namespace,
            pod_info.containers.len()
        );

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
                        let line_count = logs.lines().count();
                        info!("Retrieved {} lines of logs", line_count);

                        for (idx, line) in logs.lines().take(5).enumerate() {
                            // Limit output for demo
                            info!("LOG[{}]: {}", idx + 1, line);
                        }
                        if line_count > 5 {
                            info!("... ({} more lines)", line_count - 5);
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

        info!("Completed log tailing for pod '{}'", pod_info.name);
        Ok(())
    }
}
