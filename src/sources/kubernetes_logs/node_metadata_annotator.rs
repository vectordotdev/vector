//! Annotates events with node metadata.

#![deny(missing_docs)]

use k8s_openapi::{api::core::v1::Node, apimachinery::pkg::apis::meta::v1::ObjectMeta};
use kube::runtime::reflector::{store::Store, ObjectRef};
use lookup::lookup_v2::{parse_path, OwnedSegment};
use serde::{Deserialize, Serialize};

use crate::event::{Event, LogEvent};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    pub node_labels: String,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            node_labels: "kubernetes.node_labels".to_owned(),
        }
    }
}

/// Annotate the event with node metadata.
pub struct NodeMetadataAnnotator {
    node_state_reader: Store<Node>,
    fields_spec: FieldsSpec,
}

impl NodeMetadataAnnotator {
    /// Create a new [`NodeMetadataAnnotator`].
    pub const fn new(node_state_reader: Store<Node>, fields_spec: FieldsSpec) -> Self {
        Self {
            node_state_reader,
            fields_spec,
        }
    }
}

impl NodeMetadataAnnotator {
    /// Annotates an event with the information from the [`Node::metadata`].
    /// The event has to have a [`VECTOR_SELF_NODE_NAME`] field set.
    pub fn annotate(&self, event: &mut Event, node: &str) -> Option<()> {
        let log = event.as_mut_log();
        let obj = ObjectRef::<Node>::new(node);
        let resource = self.node_state_reader.get(&obj)?;
        let node: &Node = resource.as_ref();

        annotate_from_metadata(log, &self.fields_spec, &node.metadata);
        Some(())
    }
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    // Calculate and cache the prefix path.
    let prefix_path = parse_path(&fields_spec.node_labels);
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
                    log.insert("kubernetes.node_labels.\"sandbox0-label0\"", "val0");
                    log.insert("kubernetes.node_labels.\"sandbox0-label1\"", "val1");
                    log
                },
            ),
            (
                FieldsSpec {
                    node_labels: "node_labels".to_owned(),
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
                    log.insert("node_labels.\"sandbox0-label0\"", "val0");
                    log.insert("node_labels.\"sandbox0-label1\"", "val1");
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
