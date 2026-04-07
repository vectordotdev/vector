use std::collections::HashMap;

use bytes::Bytes;
use chrono::{DateTime, Utc};
use k8s_openapi::api::core::v1::Pod;
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    lookup::{owned_value_path, path},
};

use crate::{
    config::log_schema,
    event::{Event, LogEvent, Value},
    sources::kubernetes_logs::transform_utils::get_message_path,
};

// Re-export shared kubernetes helpers so that source.rs can import them from util
pub(super) use crate::sources::kubernetes_logs::{
    get_page_size, prepare_field_selector, prepare_label_selector, prepare_node_selector,
};

use super::Config;
use super::stream::StreamTarget;

pub(super) fn apply_pod_event(pods: &mut HashMap<String, Pod>, pod: Pod) {
    let key = pod_key(&pod);
    let is_running = pod
        .status
        .as_ref()
        .and_then(|status| status.phase.as_deref())
        == Some("Running");

    if is_running {
        if let Some(key) = key {
            pods.insert(key, pod);
        }
    } else if let Some(key) = key {
        pods.remove(&key);
    }
}

pub(super) fn remove_pod(pods: &mut HashMap<String, Pod>, pod: &Pod) {
    if let Some(key) = pod_key(pod) {
        pods.remove(&key);
    }
}

fn pod_key(pod: &Pod) -> Option<String> {
    let name = pod.metadata.name.as_deref()?;
    let namespace = pod.metadata.namespace.as_deref()?;
    Some(pod_key_from_pod_name(namespace, name))
}

pub(super) fn pod_key_from_pod_name(namespace: &str, pod_name: &str) -> String {
    format!("{namespace}/{pod_name}")
}

pub(super) fn stream_targets_for_pod(
    pod: &Pod,
    configured_container: &Option<String>,
) -> Vec<StreamTarget> {
    let Some(namespace) = pod.metadata.namespace.clone() else {
        return Vec::new();
    };
    let Some(pod_name) = pod.metadata.name.clone() else {
        return Vec::new();
    };
    let Some(spec) = pod.spec.as_ref() else {
        return Vec::new();
    };

    spec.containers
        .iter()
        .filter(|container| {
            configured_container
                .as_ref()
                .map(|configured| configured == &container.name)
                .unwrap_or(true)
        })
        .map(|container| StreamTarget {
            key: format!("{namespace}/{pod_name}/{}", container.name),
            namespace: namespace.clone(),
            pod_name: pod_name.clone(),
            container_name: container.name.clone(),
        })
        .collect()
}

pub(super) fn log_url(target: &StreamTarget, tail_lines: i64, since_seconds: i64) -> String {
    let mut url = format!(
        "/api/v1/namespaces/{}/pods/{}/log?follow=true&timestamps=true&container={}",
        target.namespace, target.pod_name, target.container_name
    );
    if tail_lines > 0 {
        url.push_str(&format!("&tailLines={tail_lines}"));
    } else if since_seconds > 0 {
        url.push_str(&format!("&sinceSeconds={since_seconds}"));
    }
    url
}

pub(super) fn split_timestamped_line(line: &str) -> (&str, &str) {
    match line.find(' ') {
        Some(index) => (&line[..index], &line[index + 1..]),
        None => (line, ""),
    }
}

pub(super) fn create_event(
    message: &str,
    timestamp: DateTime<Utc>,
    stream: &str,
    ingestion_timestamp_field: Option<&vector_lib::lookup::OwnedTargetPath>,
    log_namespace: LogNamespace,
) -> Event {
    let bytes = Bytes::copy_from_slice(message.as_bytes());
    let mut log = LogEvent::default();
    log.insert(&get_message_path(log_namespace), Value::Bytes(bytes));
    let legacy_timestamp_key = log_schema().timestamp_key().map(LegacyKey::Overwrite);
    log_namespace.insert_source_metadata(
        Config::NAME,
        &mut log,
        legacy_timestamp_key,
        path!("timestamp"),
        timestamp,
    );
    let stream_key = owned_value_path!("stream");
    log_namespace.insert_source_metadata(
        Config::NAME,
        &mut log,
        Some(LegacyKey::Overwrite(&stream_key)),
        path!("stream"),
        stream,
    );
    log_namespace.insert_vector_metadata(
        &mut log,
        log_schema().source_type_key(),
        path!("source_type"),
        Bytes::from_static(Config::NAME.as_bytes()),
    );

    match (log_namespace, ingestion_timestamp_field) {
        (LogNamespace::Vector, _) => {
            log.metadata_mut()
                .value_mut()
                .insert(path!("vector", "ingest_timestamp"), Utc::now());
        }
        (LogNamespace::Legacy, Some(field)) => {
            log.try_insert(field, Utc::now());
        }
        (LogNamespace::Legacy, None) => {}
    }

    log.into()
}

#[cfg(test)]
mod tests {
    use k8s_openapi::{
        api::core::v1::{Container, PodSpec, PodStatus},
        apimachinery::pkg::apis::meta::v1::ObjectMeta,
    };
    use vector_lib::config::log_schema;

    use super::*;

    #[test]
    fn split_timestamped_line_parses_message() {
        let (ts, msg) = split_timestamped_line("2026-03-26T12:00:00Z hello");
        assert_eq!(ts, "2026-03-26T12:00:00Z");
        assert_eq!(msg, "hello");
    }

    #[test]
    fn stream_targets_follow_container_selection() {
        let pod = Pod {
            metadata: ObjectMeta {
                name: Some("pod".to_string()),
                namespace: Some("ns".to_string()),
                ..ObjectMeta::default()
            },
            spec: Some(PodSpec {
                containers: vec![
                    Container {
                        name: "app".to_string(),
                        ..Container::default()
                    },
                    Container {
                        name: "sidecar".to_string(),
                        ..Container::default()
                    },
                ],
                ..PodSpec::default()
            }),
            status: Some(PodStatus {
                phase: Some("Running".to_string()),
                ..PodStatus::default()
            }),
        };

        assert_eq!(stream_targets_for_pod(&pod, &None).len(), 2);
        assert_eq!(
            stream_targets_for_pod(&pod, &Some("app".to_string()))
                .into_iter()
                .map(|target| target.container_name)
                .collect::<Vec<_>>(),
            vec!["app".to_string()]
        );
    }

    #[test]
    fn create_event_sets_stream_and_timestamp() {
        let event = create_event(
            "hello world",
            chrono::DateTime::parse_from_rfc3339("2026-03-26T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            "stdout",
            None,
            LogNamespace::Legacy,
        );
        let log = event.as_log();

        assert_eq!(log["message"], "hello world".into());
        assert_eq!(log["stream"], "stdout".into());
        assert_eq!(
            log[log_schema().timestamp_key().unwrap().to_string()],
            "2026-03-26T12:00:00Z".into()
        );
    }

    #[test]
    fn create_event_vector_namespace_sets_vector_metadata() {
        let event = create_event(
            "hello vector",
            chrono::DateTime::parse_from_rfc3339("2026-03-26T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
            "stderr",
            None,
            LogNamespace::Vector,
        );
        let log = event.as_log();

        assert_eq!(log.value(), &vrl::value!("hello vector"));
        assert_eq!(
            log.metadata()
                .value()
                .get(&owned_value_path!("vector", "source_type")),
            Some(&vrl::value!(Config::NAME))
        );
        assert_eq!(
            log.metadata()
                .value()
                .get(&owned_value_path!("kubernetes_logs_api", "stream")),
            Some(&vrl::value!("stderr"))
        );
    }

    #[test]
    fn apply_pod_event_tracks_only_running_pods() {
        let mut pods = HashMap::new();

        let running_pod = Pod {
            metadata: ObjectMeta {
                name: Some("pod".to_string()),
                namespace: Some("ns".to_string()),
                ..ObjectMeta::default()
            },
            status: Some(PodStatus {
                phase: Some("Running".to_string()),
                ..PodStatus::default()
            }),
            ..Pod::default()
        };
        apply_pod_event(&mut pods, running_pod.clone());
        assert!(pods.contains_key("ns/pod"));

        let pending_pod = Pod {
            status: Some(PodStatus {
                phase: Some("Pending".to_string()),
                ..PodStatus::default()
            }),
            ..running_pod
        };
        apply_pod_event(&mut pods, pending_pod);
        assert!(!pods.contains_key("ns/pod"));
    }
}
