//! Annotates events with namespace metadata.

#![deny(missing_docs)]

use k8s_openapi::{api::core::v1::Namespace, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use kube::runtime::reflector::{store::Store, ObjectRef};
use serde::{Deserialize, Serialize};

use crate::event::{Event, LogEvent};
use lookup::lookup_v2::{parse_path, OwnedSegment};

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
    namespace_state_reader: Store<Namespace>,
    fields_spec: FieldsSpec,
}

impl NamespaceMetadataAnnotator {
    /// Create a new [`NamespaceMetadataAnnotator`].
    pub const fn new(namespace_state_reader: Store<Namespace>, fields_spec: FieldsSpec) -> Self {
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
        let obj = ObjectRef::<Namespace>::new(pod_namespace);
        let resource = self.namespace_state_reader.get(&obj)?;
        let namespace: &Namespace = resource.as_ref();

        annotate_from_metadata(log, &self.fields_spec, &namespace.metadata);
        Some(())
    }
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    // Calculate and cache the prefix path.
    let prefix_path = parse_path(&fields_spec.namespace_labels);
    if let Some(labels) = &metadata.labels {
        for (key, val) in labels.iter() {
            let mut path = prefix_path.clone().segments;
            path.push(OwnedSegment::Field(key.clone()));
            log.insert(&path, val.to_owned());
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
                    log.insert("kubernetes.namespace_labels.\"sandbox0-label0\"", "val0");
                    log.insert("kubernetes.namespace_labels.\"sandbox0-label1\"", "val1");
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
                    log.insert("ns_labels.\"sandbox0-label0\"", "val0");
                    log.insert("ns_labels.\"sandbox0-label1\"", "val1");
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
                    log.insert(r#"kubernetes.namespace_labels."nested0.label0""#, "val0");
                    log.insert(r#"kubernetes.namespace_labels."nested0.label1""#, "val1");
                    log.insert(r#"kubernetes.namespace_labels."nested1.label0""#, "val2");
                    log.insert(
                        r#"kubernetes.namespace_labels."nested2.label0.deep0""#,
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
