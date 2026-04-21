use std::collections::{HashMap, HashSet};

use futures::{StreamExt, io::AsyncBufReadExt};
use k8s_openapi::api::core::v1::Pod;
use kube::Client;
use tokio::{
    sync::{Semaphore, mpsc},
    task::JoinHandle,
};
use vector_lib::config::LogNamespace;

use crate::{
    event::Event,
    sources::kubernetes_logs::{
        namespace_metadata_annotator::NamespaceMetadataAnnotator,
        node_metadata_annotator::NodeMetadataAnnotator,
        pod_metadata_annotator::{FieldsSpec as PodFieldsSpec, annotate_event_from_pod},
    },
};

use super::Config;
use super::util::{
    create_event, log_url, pod_key_from_pod_name, split_timestamped_line, stream_targets_for_pod,
};

use http_1::Request;

#[derive(Clone, Debug)]
pub(super) struct StreamTarget {
    pub(super) key: String,
    pub(super) namespace: String,
    pub(super) pod_name: String,
    pub(super) container_name: String,
    pub(super) stream: &'static str,
}

pub(super) struct StreamRuntime<'a> {
    pub(super) client: &'a Client,
    pub(super) semaphore: Option<&'a std::sync::Arc<Semaphore>>,
    pub(super) tx: &'a mpsc::Sender<Event>,
    pub(super) pod_fields_spec: &'a PodFieldsSpec,
    pub(super) namespace_annotator: &'a NamespaceMetadataAnnotator,
    pub(super) node_annotator: &'a NodeMetadataAnnotator,
    pub(super) self_node_name: &'a str,
    pub(super) tail_lines: i64,
    pub(super) since_seconds: i64,
    pub(super) ingestion_timestamp_field: Option<&'a vector_lib::lookup::OwnedTargetPath>,
    pub(super) log_namespace: LogNamespace,
}

pub(super) struct StreamTaskContext<'a> {
    pub(super) pod_fields_spec: PodFieldsSpec,
    pub(super) namespace_annotator: NamespaceMetadataAnnotator,
    pub(super) node_annotator: NodeMetadataAnnotator,
    pub(super) self_node_name: String,
    pub(super) ingestion_timestamp_field: Option<vector_lib::lookup::OwnedTargetPath>,
    pub(super) log_namespace: LogNamespace,
    pub(super) tx: mpsc::Sender<Event>,
    pub(super) _marker: std::marker::PhantomData<&'a ()>,
}

pub(super) fn reconcile_active_streams(
    pods: &HashMap<String, Pod>,
    configured_container: &Option<String>,
    active: &mut HashMap<String, JoinHandle<()>>,
    initialized: &mut HashSet<String>,
    runtime: StreamRuntime<'_>,
) {
    active.retain(|_, handle| !handle.is_finished());

    let desired: Vec<(String, StreamTarget, Pod)> = pods
        .values()
        .flat_map(|pod| stream_targets_for_pod(pod, configured_container))
        .map(|target| {
            let pod = pods[&pod_key_from_pod_name(&target.namespace, &target.pod_name)].clone();
            (target.key.clone(), target, pod)
        })
        .collect();

    let desired_keys: HashSet<_> = desired.iter().map(|(key, _, _)| key.clone()).collect();
    let stale_keys: Vec<_> = active
        .keys()
        .filter(|key| !desired_keys.contains(*key))
        .cloned()
        .collect();
    for key in stale_keys {
        if let Some(handle) = active.remove(&key) {
            handle.abort();
        }
    }

    for (key, target, pod) in desired {
        if active.contains_key(&key) {
            continue;
        }

        let permit = match runtime.semaphore {
            Some(semaphore) => match std::sync::Arc::clone(semaphore).try_acquire_owned() {
                Ok(permit) => Some(permit),
                Err(_) => continue,
            },
            None => None,
        };

        let (tail, since) = if initialized.contains(&key) {
            (0, runtime.since_seconds)
        } else {
            (runtime.tail_lines, runtime.since_seconds)
        };

        let client = runtime.client.clone();
        let target_clone = target.clone();
        let task_context = StreamTaskContext {
            pod_fields_spec: runtime.pod_fields_spec.clone(),
            namespace_annotator: runtime.namespace_annotator.clone(),
            node_annotator: runtime.node_annotator.clone(),
            self_node_name: runtime.self_node_name.to_owned(),
            ingestion_timestamp_field: runtime.ingestion_timestamp_field.cloned(),
            log_namespace: runtime.log_namespace,
            tx: runtime.tx.clone(),
            _marker: std::marker::PhantomData,
        };

        let handle = tokio::spawn(async move {
            let _permit = permit;
            if let Err(error) =
                stream_target(client, pod, target_clone, tail, since, task_context).await
            {
                warn!(message = "Pod log stream terminated with an error.", %error);
            }
        });
        initialized.insert(key.clone());
        active.insert(key, handle);
    }
}

async fn stream_target(
    client: Client,
    pod: Pod,
    target: StreamTarget,
    tail_lines: i64,
    since_seconds: i64,
    context: StreamTaskContext<'_>,
) -> crate::Result<()> {
    let request = Request::get(log_url(&target, tail_lines, since_seconds)).body(vec![])?;
    let byte_stream = client.request_stream(request).await?;
    let mut lines = byte_stream.lines();
    let mut last_ts = String::new();

    loop {
        match lines.next().await {
            Some(Ok(line)) => {
                let (timestamp_raw, message) = split_timestamped_line(&line);
                if timestamp_raw < last_ts.as_str() {
                    continue;
                }
                last_ts = timestamp_raw.to_owned();

                let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp_raw)
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_else(|_| chrono::Utc::now());
                let mut event = create_event(
                    message,
                    timestamp,
                    target.stream,
                    context.ingestion_timestamp_field.as_ref(),
                    context.log_namespace,
                );

                annotate_event_from_pod(
                    event.as_mut_log(),
                    &context.pod_fields_spec,
                    &pod,
                    Some(&target.container_name),
                    context.log_namespace,
                    Config::NAME,
                );

                if context
                    .namespace_annotator
                    .annotate(&mut event, &target.namespace)
                    .is_none()
                {
                    trace!(message = "Namespace metadata not available yet.", namespace = %target.namespace);
                }
                if context
                    .node_annotator
                    .annotate(&mut event, &context.self_node_name)
                    .is_none()
                {
                    trace!(message = "Node metadata not available yet.", node = %context.self_node_name);
                }

                if context.tx.send(event).await.is_err() {
                    break;
                }
            }
            Some(Err(error)) => return Err(Box::new(error)),
            None => break,
        }
    }

    Ok(())
}
