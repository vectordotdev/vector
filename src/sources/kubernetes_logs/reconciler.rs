use super::pod_info::PodInfo;
use bytes::Bytes;
use chrono::{DateTime, FixedOffset, Utc};
use futures::SinkExt;
use futures::channel::mpsc;
use futures::{AsyncBufReadExt, StreamExt};
use futures_util::Stream;
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher;
use kube::{Api, Client, api::LogParams};
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use tracing::{info, trace, warn};

/// Log line with associated container metadata
#[derive(Clone, Debug)]
pub struct LogWithMetadata {
    /// The actual log content
    pub log_line: Bytes,
    /// Pod name
    pub pod_name: String,
    /// Namespace name
    pub namespace_name: String,
    /// Container name
    pub container_name: String,
}

/// Container key for identifying unique container instances
/// Format: "{namespace}/{pod_name}/{container_name}"
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ContainerKey(String);

impl fmt::Display for ContainerKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&ContainerInfo> for ContainerKey {
    fn from(container_info: &ContainerInfo) -> Self {
        ContainerKey(format!(
            "{}/{}/{}",
            container_info.namespace, container_info.pod_name, container_info.container_name
        ))
    }
}

impl From<(&PodInfo, &str)> for ContainerKey {
    fn from((pod_info, container_name): (&PodInfo, &str)) -> Self {
        ContainerKey(format!(
            "{}/{}/{}",
            pod_info.namespace, pod_info.name, container_name
        ))
    }
}

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
    fn update_last_log_timestamp(&mut self, log_line: &[u8]) -> bool {
        // Kubernetes log format typically starts with RFC3339 timestamp
        // e.g., "2023-10-11T10:30:00.123456789Z message content"
        if let Some(timestamp_end) = log_line.iter().position(|&b| b == b' ') {
            let timestamp_bytes = &log_line[..timestamp_end];
            if let Ok(timestamp_str) = std::str::from_utf8(timestamp_bytes) {
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
                        let timestamp = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc)
                            .fixed_offset();
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
        }
        false
    }
}

pub struct Reconciler {
    esb: EventStreamBuilder,
    states: HashMap<ContainerKey, TailerState>, // Keyed by ContainerKey
    pod_watcher: Pin<Box<dyn Stream<Item = watcher::Result<watcher::Event<Pod>>> + Send>>,
}

impl Reconciler {
    pub fn new<S>(
        client: Client,
        log_sender: mpsc::UnboundedSender<LogWithMetadata>,
        pod_watcher: S,
    ) -> Self
    where
        S: Stream<Item = watcher::Result<watcher::Event<Pod>>> + Send + 'static,
    {
        let esb = EventStreamBuilder {
            client: client.clone(),
            log_sender,
        };
        Self {
            esb,
            states: HashMap::new(),
            pod_watcher: Box::pin(pod_watcher),
        }
    }

    pub async fn run(mut self) {
        info!("Starting reconciler with pod watcher integration");

        // Listen to pod watcher events for real-time reconciliation
        while let Some(event) = self.pod_watcher.next().await {
            match event {
                Ok(watcher::Event::Delete(pod)) => {
                    let pod_info = PodInfo::from(&pod);
                    info!("Pod '{}' deleted, cleaning up log tailers", pod_info.name);
                    self.cleanup_pod_tailers(&pod_info).await;
                }
                Ok(watcher::Event::InitApply(pod)) | Ok(watcher::Event::Apply(pod)) => {
                    let pod_info = PodInfo::from(&pod);
                    if let Some(phase) = &pod_info.phase {
                        if phase == "Running" {
                            info!(
                                "Pod '{}' is running, starting log reconciliation",
                                pod_info.name
                            );
                            if let Err(e) = self.reconcile_pod_containers(&pod_info).await {
                                warn!("Failed to reconcile pod '{}': {}", pod_info.name, e);
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    warn!("Pod watcher error: {}", e);
                }
            }
        }

        info!("Reconciler pod watcher stream ended");
    }

    /// Reconcile containers for a specific pod
    async fn reconcile_pod_containers(&mut self, pod_info: &PodInfo) -> crate::Result<()> {
        for container_name in &pod_info.containers {
            let container_info = ContainerInfo {
                pod_name: pod_info.name.clone(),
                namespace: pod_info.namespace.clone(),
                container_name: container_name.clone(),
                pod_uid: pod_info.uid.clone(),
            };

            let key = ContainerKey::from(&container_info);

            // Only start tailer if not already running
            if !self.states.contains_key(&key) {
                info!(
                    "Starting tailer for container '{}' in pod '{}' (namespace '{}')",
                    container_info.container_name,
                    container_info.pod_name,
                    container_info.namespace
                );

                self.states.insert(key, self.esb.start(container_info));
            }
        }
        Ok(())
    }

    /// Clean up tailers for a deleted pod
    async fn cleanup_pod_tailers(&mut self, pod_info: &PodInfo) {
        for container_name in &pod_info.containers {
            let key = ContainerKey::from((pod_info, container_name.as_str()));

            if self.states.remove(&key).is_some() {
                info!(
                    "Cleaned up tailer for container '{}' in deleted pod '{}'",
                    container_name, pod_info.name
                );
            }
        }
    }
}

#[derive(Clone)]
struct EventStreamBuilder {
    client: Client,
    log_sender: mpsc::UnboundedSender<LogWithMetadata>,
}

#[derive(Clone)]
enum TailerState {
    Running,
    // Stopped,
}

impl EventStreamBuilder {
    pub fn start(&self, container_info: ContainerInfo) -> TailerState {
        let this = self.clone();
        tokio::spawn(async move {
            let log_info = ContainerLogInfo::new(container_info, Utc::now());
            this.run_event_stream(log_info).await;
            return;
        });
        TailerState::Running
    }

    pub async fn run_event_stream(mut self, mut log_info: ContainerLogInfo) {
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

        match pods
            .log_stream(&log_info.container_info.pod_name, &log_params)
            .await
        {
            Ok(log_stream) => {
                info!(
                    "Started streaming logs from container '{}' in pod '{}'",
                    log_info.container_info.container_name, log_info.container_info.pod_name
                );

                let mut buffer = Vec::new();
                let mut log_stream = log_stream;

                // Process the stream by reading line by line
                loop {
                    buffer.clear();
                    match log_stream.read_until(b'\n', &mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Remove trailing newline if present
                            if buffer.ends_with(&[b'\n']) {
                                buffer.pop();
                            }
                            // Remove trailing carriage return if present (for CRLF)
                            if buffer.ends_with(&[b'\r']) {
                                buffer.pop();
                            }

                            let line_bytes = Bytes::from(buffer.clone());

                            // Update timestamp tracking before sending
                            let timestamp_updated = log_info.update_last_log_timestamp(&buffer);
                            if timestamp_updated {
                                trace!(
                                    "Updated last log timestamp for container '{}' in pod '{}' to: {:?}",
                                    log_info.container_info.container_name,
                                    log_info.container_info.pod_name,
                                    log_info.last_log
                                );
                            }

                            // Send the log line with metadata to the channel
                            let log_with_metadata = LogWithMetadata {
                                log_line: line_bytes,
                                pod_name: log_info.container_info.pod_name.clone(),
                                namespace_name: log_info.container_info.namespace.clone(),
                                container_name: log_info.container_info.container_name.clone(),
                            };

                            if let Err(_) = self.log_sender.send(log_with_metadata).await {
                                warn!(
                                    "Log channel closed for container '{}' in pod '{}', stopping stream",
                                    log_info.container_info.container_name,
                                    log_info.container_info.pod_name
                                );
                                break;
                            }
                        }
                        Err(e) => {
                            warn!(
                                "Error reading from log stream for container '{}' in pod '{}': {}",
                                log_info.container_info.container_name,
                                log_info.container_info.pod_name,
                                e
                            );
                            break;
                        }
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
    }
}
