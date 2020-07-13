//! Annotates events with pod metadata.

#![deny(missing_docs)]

use super::path_helpers::parse_log_file_path;
use crate::{event::LogEvent, Event};
use evmap10::ReadHandle;
use k8s_openapi::{api::core::v1::Pod, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    pub pod_name: String,
    pub pod_namespace: String,
    pub pod_uid: String,
    pub pod_labels: String,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            pod_name: "kubernetes.pod_name".to_owned(),
            pod_namespace: "kubernetes.pod_namespace".to_owned(),
            pod_uid: "kubernetes.pod_uid".to_owned(),
            pod_labels: "kubernetes.pod_labels".to_owned(),
        }
    }
}

/// Annotate the event with pod metadata.
pub struct PodMetadataAnnotator {
    pods_state_reader: ReadHandle<String, k8s_runtime::state::evmap::Value<Pod>>,
    fields_spec: FieldsSpec,
}

impl PodMetadataAnnotator {
    /// Create a new [`PodMetadataAnnotator`].
    pub fn new(
        pods_state_reader: ReadHandle<String, k8s_runtime::state::evmap::Value<Pod>>,
        fields_spec: FieldsSpec,
    ) -> Self {
        Self {
            pods_state_reader,
            fields_spec,
        }
    }
}

impl PodMetadataAnnotator {
    /// Annotates an event with the information from the [`Pod::metadata`].
    /// The event has to be obtained from kubernetes log file, and have a
    /// [`FILE_KEY`] field set with a file that the line came from.
    pub fn annotate(&self, event: &mut Event, file: &str) -> Option<()> {
        let log = event.as_mut_log();
        let uid = parse_log_file_path(file)?.pod_uid;
        let guard = self.pods_state_reader.get(uid)?;
        let entry = guard.get_one()?;
        let pod: &Pod = entry.as_ref();
        annotate_from_metadata(log, &self.fields_spec, &pod.metadata);
        Some(())
    }
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    for (ref key, ref val) in [
        (&fields_spec.pod_name, &metadata.name),
        (&fields_spec.pod_namespace, &metadata.namespace),
        (&fields_spec.pod_uid, &metadata.uid),
    ]
    .iter()
    {
        if let Some(val) = val {
            log.insert(key, val);
        }
    }

    if let Some(labels) = &metadata.labels {
        for (key, val) in labels.iter() {
            log.insert(format!("{}.{}", fields_spec.pod_labels, key), val);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_annotate_from_metadata() {
        let cases = vec![
            (
                FieldsSpec::default(),
                ObjectMeta::default(),
                LogEvent::default(),
            ),
            (
                FieldsSpec::default(),
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("sandbox0-label0".to_owned(), "val0".to_owned()),
                            ("sandbox0-label1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("kubernetes.pod_name", "sandbox0-name");
                    log.insert("kubernetes.pod_namespace", "sandbox0-ns");
                    log.insert("kubernetes.pod_uid", "sandbox0-uid");
                    log.insert("kubernetes.pod_labels.sandbox0-label0", "val0");
                    log.insert("kubernetes.pod_labels.sandbox0-label1", "val1");
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_name: "name".to_owned(),
                    pod_namespace: "ns".to_owned(),
                    pod_uid: "uid".to_owned(),
                    pod_labels: "labels".to_owned(),
                },
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("sandbox0-label0".to_owned(), "val0".to_owned()),
                            ("sandbox0-label1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("name", "sandbox0-name");
                    log.insert("ns", "sandbox0-ns");
                    log.insert("uid", "sandbox0-uid");
                    log.insert("labels.sandbox0-label0", "val0");
                    log.insert("labels.sandbox0-label1", "val1");
                    log
                },
            ),
        ];

        for (fields_spec, metadata, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_metadata(&mut log, &fields_spec, &metadata);
            assert_eq!(log, expected);
        }
    }
}
