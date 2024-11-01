//! Annotates events with namespace metadata.

#![deny(missing_docs)]

use k8s_openapi::{api::core::v1::Namespace, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use kube::runtime::reflector::{store::Store, ObjectRef};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;
use vector_lib::lookup::{lookup_v2::ValuePath, owned_value_path, path, OwnedTargetPath};

use crate::event::{Event, LogEvent};

use super::Config;

/// Configuration for how the events are enriched with Namespace metadata.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    /// Event field for the Namespace's labels.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.ns_labels"))]
    #[configurable(metadata(docs::examples = "k8s.ns_labels"))]
    #[configurable(metadata(docs::examples = ""))]
    pub namespace_labels: OptionalTargetPath,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            namespace_labels: OwnedTargetPath::event(owned_value_path!(
                "kubernetes",
                "namespace_labels"
            ))
            .into(),
        }
    }
}

/// Annotate the event with namespace metadata.
pub struct NamespaceMetadataAnnotator {
    namespace_state_reader: Store<Namespace>,
    fields_spec: FieldsSpec,
    log_namespace: LogNamespace,
}

impl NamespaceMetadataAnnotator {
    /// Create a new [`NamespaceMetadataAnnotator`].
    pub const fn new(
        namespace_state_reader: Store<Namespace>,
        fields_spec: FieldsSpec,
        log_namespace: LogNamespace,
    ) -> Self {
        Self {
            namespace_state_reader,
            fields_spec,
            log_namespace,
        }
    }
}

impl NamespaceMetadataAnnotator {
    /// Annotates an event with the information from the [`Namespace::metadata`].
    pub fn annotate(&self, event: &mut Event, pod_namespace: &str) -> Option<()> {
        let log = event.as_mut_log();
        let obj = ObjectRef::<Namespace>::new(pod_namespace);
        let resource = self.namespace_state_reader.get(&obj)?;
        let namespace: &Namespace = resource.as_ref();

        annotate_from_metadata(
            log,
            &self.fields_spec,
            &namespace.metadata,
            self.log_namespace,
        );
        Some(())
    }
}

fn annotate_from_metadata(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    metadata: &ObjectMeta,
    log_namespace: LogNamespace,
) {
    if let Some(labels) = &metadata.labels {
        if let Some(prefix_path) = &fields_spec.namespace_labels.path {
            for (key, value) in labels.iter() {
                let key_path = path!(key);

                log_namespace.insert_source_metadata(
                    Config::NAME,
                    log,
                    Some(LegacyKey::Overwrite((&prefix_path.path).concat(key_path))),
                    path!("namespace_labels", key),
                    value.to_owned(),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use similar_asserts::assert_eq;
    use vector_lib::lookup::{event_path, metadata_path};

    use super::*;

    #[test]
    fn test_annotate_from_metadata() {
        let cases = vec![
            (
                FieldsSpec::default(),
                ObjectMeta::default(),
                LogEvent::default(),
                LogNamespace::Legacy,
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
                    log.insert(
                        metadata_path!("kubernetes_logs", "namespace_labels", "sandbox0-label0"),
                        "val0",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "namespace_labels", "sandbox0-label1"),
                        "val1",
                    );
                    log
                },
                LogNamespace::Vector,
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
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "sandbox0-label0"),
                        "val0",
                    );
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "sandbox0-label1"),
                        "val1",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    namespace_labels: OwnedTargetPath::event(owned_value_path!("ns_labels")).into(),
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
                    log.insert(event_path!("ns_labels", "sandbox0-label0"), "val0");
                    log.insert(event_path!("ns_labels", "sandbox0-label1"), "val1");
                    log
                },
                LogNamespace::Legacy,
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
                    log.insert(
                        metadata_path!("kubernetes_logs", "namespace_labels", "nested0.label0"),
                        "val0",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "namespace_labels", "nested0.label1"),
                        "val1",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "namespace_labels", "nested1.label0"),
                        "val2",
                    );
                    log.insert(
                        metadata_path!(
                            "kubernetes_logs",
                            "namespace_labels",
                            "nested2.label0.deep0"
                        ),
                        "val3",
                    );
                    log
                },
                LogNamespace::Vector,
            ),
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
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "nested0.label0"),
                        "val0",
                    );
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "nested0.label1"),
                        "val1",
                    );
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "nested1.label0"),
                        "val2",
                    );
                    log.insert(
                        event_path!("kubernetes", "namespace_labels", "nested2.label0.deep0"),
                        "val3",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
        ];

        for (fields_spec, metadata, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_metadata(&mut log, &fields_spec, &metadata, log_namespace);
            assert_eq!(log, expected);
        }
    }
}
