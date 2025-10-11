use super::pod_info::PodInfo;
use chrono::{DateTime, FixedOffset, Utc};
use futures::SinkExt;
use futures::channel::mpsc;
use futures::{AsyncBufReadExt, TryStreamExt};
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::reflector::Store;
use kube::{Api, Client, api::LogParams};
use std::collections::HashMap;
use tracing::{error, info, trace, warn};

/// Container information for log tailing
#[derive(Clone, Debug)]
pub struct ContainerInfo {
    /// Pod name containing this container
    pub pod_name: String,
    /// Pod namespace
    pub namespace: String,
    /// Container name
    pub container_name: String,
    /// Pod UID for tracking (will be used for future state tracking)
    #[allow(dead_code)]
    pub pod_uid: String,
}

/// Container log information with timestamp tracking
/// Similar to docker_logs ContainerLogInfo for position tracking
#[derive(Clone, Debug)]
struct ContainerLogInfo {
    /// Container information
    container_info: ContainerInfo,
    /// Timestamp of when this tracking started
    created: DateTime<Utc>,
    /// Timestamp of last log message processed
    last_log: Option<DateTime<FixedOffset>>,
}

impl ContainerLogInfo {
    fn new(container_info: ContainerInfo, created: DateTime<Utc>) -> Self {
        Self {
            container_info,
            created,
            last_log: None,
        }
    }

    /// Get the timestamp from which logs should be fetched
    /// Only logs after this point need to be fetched
    fn log_since(&self) -> DateTime<Utc> {
        self.last_log
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(self.created)
    }

    /// Update the last log timestamp when processing a log line
    /// Returns true if the timestamp was successfully parsed and updated
    fn update_last_log_timestamp(&mut self, log_line: &str) -> bool {
        // Kubernetes log format typically starts with RFC3339 timestamp
        // e.g., "2023-10-11T10:30:00.123456789Z message content"
        if let Some(timestamp_end) = log_line.find(' ') {
            let timestamp_str = &log_line[..timestamp_end];
            if let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp_str) {
                // Only update if this timestamp is newer than our last recorded timestamp
                if let Some(last) = self.last_log {
                    if timestamp > last {
                        self.last_log = Some(timestamp);
                        return true;
                    }
                } else {
                    // First timestamp we've seen
                    self.last_log = Some(timestamp);
                    return true;
                }
            } else {
                // Try to parse ISO 8601 format without timezone (common in k8s logs)
                if let Ok(naive_dt) =
                    chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%dT%H:%M:%S%.f")
                {
                    let timestamp =
                        DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc).fixed_offset();
                    if let Some(last) = self.last_log {
                        if timestamp > last {
                            self.last_log = Some(timestamp);
                            return true;
                        }
                    } else {
                        self.last_log = Some(timestamp);
                        return true;
                    }
                }
            }
        }
        false
    }
}

pub struct Reconciler {
    pod_state: Store<Pod>,
    container_tailer: ContainerLogTailer,
    tailer_state: HashMap<String, ContainerLogInfo>, // Keyed by "namespace/pod/container"
}

impl Reconciler {
    pub fn new(
        pod_state: Store<Pod>,
        client: Client,
        log_sender: mpsc::UnboundedSender<String>,
    ) -> Self {
        let container_tailer = ContainerLogTailer::new(client.clone(), log_sender);
        Self {
            pod_state,
            container_tailer,
            tailer_state: HashMap::new(),
        }
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

    pub async fn handle_running_pods(mut self) -> crate::Result<Self> {
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
            // Convert pods to container info and start tailers
            let containers: Vec<ContainerInfo> = running_pods
                .iter()
                .flat_map(|pod_info| {
                    pod_info
                        .containers
                        .iter()
                        .map(|container_name| ContainerInfo {
                            pod_name: pod_info.name.clone(),
                            namespace: pod_info.namespace.clone(),
                            container_name: container_name.clone(),
                            pod_uid: pod_info.uid.clone(),
                        })
                })
                .collect();

            info!(
                "Starting log tailing for {} containers across {} running pods",
                containers.len(),
                running_pods.len()
            );

            for container_info in containers {
                info!(
                    "Starting tailer for container '{}' in pod '{}' (namespace '{}')",
                    container_info.container_name,
                    container_info.pod_name,
                    container_info.namespace
                );

                let key = format!(
                    "{}/{}/{}",
                    container_info.namespace,
                    container_info.pod_name,
                    container_info.container_name
                );

                // Check if we already have tracking info for this container
                let log_info = if let Some(existing_info) = self.tailer_state.get(&key) {
                    // Reuse existing timestamp tracking
                    existing_info.clone()
                } else {
                    // Create new tracking info starting from now
                    ContainerLogInfo::new(container_info.clone(), Utc::now())
                };

                self.container_tailer.start(&log_info);
                self.tailer_state.insert(key, log_info);
            }
        }

        Ok(self)
    }
}

#[derive(Clone)]
struct ContainerLogTailer {
    client: Client,
    log_sender: mpsc::UnboundedSender<String>,
}

// #[derive(Clone)]
// enum TailStatus {
//     Running,
//     // Stopped,
// }

impl ContainerLogTailer {
    pub fn new(client: Client, log_sender: mpsc::UnboundedSender<String>) -> Self {
        Self { client, log_sender }
    }

    pub fn start(&self, log_info: &ContainerLogInfo) {
        let mut log_info = log_info.clone();
        let client = self.client.clone();
        let log_sender = self.log_sender.clone();
        tokio::spawn(async move {
            let mut tailer = ContainerLogTailer { client, log_sender };
            if let Err(e) = tailer.tail_container_logs(&mut log_info).await {
                error!(
                    "Error tailing logs for container '{}' in pod '{}': {}",
                    log_info.container_info.container_name, log_info.container_info.pod_name, e
                );
            }
        });
    }

    pub async fn tail_container_logs(
        &mut self,
        log_info: &mut ContainerLogInfo,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pods: Api<Pod> =
            Api::namespaced(self.client.clone(), &log_info.container_info.namespace);

        info!(
            "Starting streaming log tail for container '{}' in pod '{}' (namespace '{}') from timestamp {}",
            log_info.container_info.container_name,
            log_info.container_info.pod_name,
            log_info.container_info.namespace,
            log_info.log_since()
        );

        let log_params = LogParams {
            container: Some(log_info.container_info.container_name.clone()),
            follow: true,
            since_time: Some(log_info.log_since()),
            timestamps: true,
            ..Default::default()
        };

        // Use log_stream for continuous streaming instead of one-shot logs
        match pods
            .log_stream(&log_info.container_info.pod_name, &log_params)
            .await
        {
            Ok(log_stream) => {
                info!(
                    "Started streaming logs from container '{}' in pod '{}'",
                    log_info.container_info.container_name, log_info.container_info.pod_name
                );

                let mut lines = log_stream.lines();
                let mut log_count = 0;

                // Process the stream of log lines continuously
                while let Some(line_result) = lines.try_next().await? {
                    // Update timestamp tracking before sending
                    let timestamp_updated = log_info.update_last_log_timestamp(&line_result);
                    if timestamp_updated {
                        trace!(
                            "Updated last log timestamp for container '{}' in pod '{}' to: {:?}",
                            log_info.container_info.container_name,
                            log_info.container_info.pod_name,
                            log_info.last_log
                        );
                    }

                    // Send the log line to the channel
                    if let Err(_) = self.log_sender.send(line_result).await {
                        warn!(
                            "Log channel closed for container '{}' in pod '{}', stopping stream",
                            log_info.container_info.container_name,
                            log_info.container_info.pod_name
                        );
                        break;
                    }

                    log_count += 1;

                    // Log progress periodically
                    if log_count % 100 == 0 {
                        trace!(
                            "Processed {} log lines from container '{}' in pod '{}'. Last timestamp: {:?}",
                            log_count,
                            log_info.container_info.container_name,
                            log_info.container_info.pod_name,
                            log_info.last_log
                        );
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to start log stream for container '{}' in pod '{}': {}",
                    log_info.container_info.container_name, log_info.container_info.pod_name, e
                );
            }
        }

        info!(
            "Completed streaming log tail for container '{}' in pod '{}'",
            log_info.container_info.container_name, log_info.container_info.pod_name
        );
        Ok(())
    }
}
