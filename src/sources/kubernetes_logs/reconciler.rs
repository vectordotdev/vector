use super::pod_info::PodInfo;
use bytes::Bytes;
use chrono::{DateTime, FixedOffset, Utc};
use futures::SinkExt;
use futures::channel::mpsc;
use futures::{AsyncBufReadExt, StreamExt};
use futures_util::{Stream, future::ready};
use k8s_openapi::api::core::v1::Pod;
use kube::runtime::watcher;
use kube::{Api, Client, api::LogParams};
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tracing::{info, warn};
use vector_lib::{file_source::file_server::Line, file_source_common::FileFingerprint};

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
}

/// Container log information with timestamp tracking
/// Similar to docker_logs ContainerLogInfo for position tracking
#[derive(Debug)]
struct ContainerLogInfo<'a> {
    /// Container information reference
    container_info: &'a ContainerInfo,
    /// Timestamp of when this tracking started
    created: DateTime<Utc>,
    /// Timestamp of last log message processed
    last_log: Option<DateTime<FixedOffset>>,
}

impl<'a> ContainerLogInfo<'a> {
    const fn new(container_info: &'a ContainerInfo, created: DateTime<Utc>) -> Self {
        Self {
            container_info,
            created,
            last_log: None,
        }
    }

    fn log_since(&self) -> DateTime<Utc> {
        self.last_log
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or(self.created)
    }
}

pub struct Reconciler {
    esb: EventStreamBuilder,
    states: HashMap<ContainerKey, TailerState>, // Keyed by ContainerKey
    pod_watcher: Pin<Box<dyn Stream<Item = watcher::Event<Pod>> + Send>>,
}

impl Reconciler {
    pub fn new(
        client: Client,
        line_sender: mpsc::Sender<Vec<Line>>,
        pod_receiver: broadcast::Receiver<watcher::Event<Pod>>,
    ) -> Self {
        let esb = EventStreamBuilder {
            client: client.clone(),
            line_sender,
        };

        let pod_stream = BroadcastStream::new(pod_receiver).filter_map(|event| ready(event.ok()));

        Self {
            esb,
            states: HashMap::new(),
            pod_watcher: Box::pin(pod_stream),
        }
    }

    pub async fn run(mut self) {
        info!("Starting reconciler with pod watcher integration");

        // Listen to pod watcher events for real-time reconciliation
        while let Some(event) = self.pod_watcher.next().await {
            match event {
                watcher::Event::Delete(pod) => {
                    let pod_info = PodInfo::from(&pod);
                    info!("Pod '{}' deleted, cleaning up log tailers", pod_info.name);
                    self.cleanup_pod_tailers(&pod_info).await;
                }
                watcher::Event::InitApply(pod) | watcher::Event::Apply(pod) => {
                    let pod_info = PodInfo::from(&pod);
                    if let Some(phase) = &pod_info.phase
                        && phase == "Running"
                    {
                        info!(
                            "Pod '{}' is running, starting log reconciliation",
                            pod_info.name
                        );
                        if let Err(e) = self.reconcile_pod_containers(&pod_info).await {
                            warn!("Failed to reconcile pod '{}': {}", pod_info.name, e);
                        }
                    }
                }
                _ => {}
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
    line_sender: mpsc::Sender<Vec<Line>>,
}

#[derive(Clone)]
enum TailerState {
    Running,
}

impl EventStreamBuilder {
    pub fn start(&self, container_info: ContainerInfo) -> TailerState {
        let this = self.clone();
        tokio::spawn(async move {
            let log_info = ContainerLogInfo::new(&container_info, Utc::now());
            this.run_event_stream(log_info).await;
        });
        TailerState::Running
    }

    pub async fn run_event_stream(mut self, log_info: ContainerLogInfo<'_>) {
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
                    match log_stream.read_until(b'\n', &mut buffer).await {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Remove trailing newline if present
                            if buffer.ends_with(b"\n") {
                                buffer.pop();
                            }
                            // Remove trailing carriage return if present (for CRLF)
                            if buffer.ends_with(b"\r") {
                                buffer.pop();
                            }

                            let line_bytes = Bytes::from(std::mem::take(&mut buffer));

                            // TODO: track last log timestamp

                            let text_len = line_bytes.len() as u64;
                            let line = Line {
                                text: line_bytes,
                                filename: String::new(), // Filename is not applicable for k8s logs
                                file_id: FileFingerprint::FirstLinesChecksum(0),
                                start_offset: 0,
                                end_offset: text_len,
                            };

                            // TODO: Send batches of lines instead of one by one
                            if self.line_sender.send(vec![line]).await.is_err() {
                                warn!(
                                    "Line channel closed for container '{}' in pod '{}', stopping stream",
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
