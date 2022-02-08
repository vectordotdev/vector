//! Annotates events with namespace metadata.

#![deny(missing_docs)]

use evmap::ReadHandle;
use k8s_openapi::{api::core::v1::Namespace, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use serde::{Deserialize, Serialize};

use crate::{
    event::{Event, LogEvent, PathComponent, PathIter},
    kubernetes as k8s,
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    pub namespace_labels: String,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            namespace_labels: "kubernetes.namespace_labels".to_owned(),
        }
    }
}

/// Annotate the event with namespace metadata.
pub struct NamespaceMetadataAnnotator {
    namespace_state_reader: ReadHandle<String, k8s::state::evmap::Value<Namespace>>,
    fields_spec: FieldsSpec,
}

impl NamespaceMetadataAnnotator {
    /// Create a new [`NamespaceMetadataAnnotator`].
    pub fn new(
        namespace_state_reader: ReadHandle<String, k8s::state::evmap::Value<Namespace>>,
        fields_spec: FieldsSpec,
    ) -> Self {
        Self {
            namespace_state_reader,
            fields_spec,
        }
    }
}

impl NamespaceMetadataAnnotator {
    /// Annotates an event with the information from the [`Namespace::metadata`].
    /// The event has to have a [`POD_NAMESPACE`] field set.
    pub fn annotate(&self, event: &mut Event, pod_namespace: &str) -> Option<()> {
        let log = event.as_mut_log();
        let guard = self.namespace_state_reader.get(pod_namespace)?;
        let entry = guard.get_one()?;
        let namespace: &Namespace = entry.as_ref();

        annotate_from_metadata(log, &self.fields_spec, &namespace.metadata);
        Some(())
    }
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    // Calculate and cache the prefix path.
    let prefix_path = PathIter::new(fields_spec.namespace_labels.as_ref()).collect::<Vec<_>>();
    if let Some(labels) = &metadata.labels {
        for (key, val) in labels.iter() {
            let mut path = prefix_path.clone();
            path.push(PathComponent::Key(key.clone().into()));
            log.insert_path(path, val.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use vector_common::assert_event_data_eq;

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
                    log.insert("kubernetes.namespace_labels.sandbox0-label0", "val0");
                    log.insert("kubernetes.namespace_labels.sandbox0-label1", "val1");
                    log
                },
            ),
            (
                FieldsSpec {
                    namespace_labels: "ns_labels".to_owned(),
                },
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
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
                    log.insert("ns_labels.sandbox0-label0", "val0");
                    log.insert("ns_labels.sandbox0-label1", "val1");
                    log
                },
            ),
            // Ensure we properly handle labels with `.` as flat fields.
            (
                FieldsSpec::default(),
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    uid: Some("sandbox0-uid".to_owned()),
                    labels: Some(
                        vec![
                            ("nested0.label0".to_owned(), "val0".to_owned()),
                            ("nested0.label1".to_owned(), "val1".to_owned()),
                            ("nested1.label0".to_owned(), "val2".to_owned()),
                            ("nested2.label0.deep0".to_owned(), "val3".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),

                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(r#"kubernetes.namespace_labels.nested0\.label0"#, "val0");
                    log.insert(r#"kubernetes.namespace_labels.nested0\.label1"#, "val1");
                    log.insert(r#"kubernetes.namespace_labels.nested1\.label0"#, "val2");
                    log.insert(
                        r#"kubernetes.namespace_labels.nested2\.label0\.deep0"#,
                        "val3",
                    );
                    log
                },
            ),
        ];

        for (fields_spec, metadata, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_metadata(&mut log, &fields_spec, &metadata);
            assert_event_data_eq!(log, expected);
        }
    }
}
