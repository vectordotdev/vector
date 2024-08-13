//! Annotates events with pod metadata.

#![deny(missing_docs)]

use k8s_openapi::{
    api::core::v1::{Container, ContainerStatus, Pod, PodSpec, PodStatus},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use kube::runtime::reflector::{store::Store, ObjectRef};
use vector_lib::config::{LegacyKey, LogNamespace};
use vector_lib::configurable::configurable_component;
use vector_lib::lookup::{
    lookup_v2::{OptionalTargetPath, ValuePath},
    owned_value_path, path, OwnedTargetPath,
};

use super::{
    path_helpers::{parse_log_file_path, LogFileInfo},
    Config,
};
use crate::event::{Event, LogEvent};

/// Configuration for how the events are enriched with Pod metadata.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    /// Event field for the Pod's name.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_name"))]
    #[configurable(metadata(docs::examples = "k8s.pod_name"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_name: OptionalTargetPath,

    /// Event field for the Pod's namespace.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_ns"))]
    #[configurable(metadata(docs::examples = "k8s.pod_ns"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_namespace: OptionalTargetPath,

    /// Event field for the Pod's UID.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_uid"))]
    #[configurable(metadata(docs::examples = "k8s.pod_uid"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_uid: OptionalTargetPath,

    /// Event field for the Pod's IPv4 address.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_ip"))]
    #[configurable(metadata(docs::examples = "k8s.pod_ip"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_ip: OptionalTargetPath,

    /// Event field for the Pod's IPv4 and IPv6 addresses.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_ips"))]
    #[configurable(metadata(docs::examples = "k8s.pod_ips"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_ips: OptionalTargetPath,

    /// Event field for the `Pod`'s labels.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_labels"))]
    #[configurable(metadata(docs::examples = "k8s.pod_labels"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_labels: OptionalTargetPath,

    /// Event field for the Pod's annotations.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_annotations"))]
    #[configurable(metadata(docs::examples = "k8s.pod_annotations"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_annotations: OptionalTargetPath,

    /// Event field for the Pod's node_name.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_host"))]
    #[configurable(metadata(docs::examples = "k8s.pod_host"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_node_name: OptionalTargetPath,

    /// Event field for the Pod's owner reference.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.pod_owner"))]
    #[configurable(metadata(docs::examples = "k8s.pod_owner"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_owner: OptionalTargetPath,

    /// Event field for the Container's name.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.container_name"))]
    #[configurable(metadata(docs::examples = "k8s.container_name"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_name: OptionalTargetPath,

    /// Event field for the Container's ID.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.container_id"))]
    #[configurable(metadata(docs::examples = "k8s.container_id"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_id: OptionalTargetPath,

    /// Event field for the Container's image.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.container_image"))]
    #[configurable(metadata(docs::examples = "k8s.container_image"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_image: OptionalTargetPath,

    /// Event field for the Container's image ID.
    ///
    /// Set to `""` to suppress this key.
    #[configurable(metadata(docs::examples = ".k8s.container_image_id"))]
    #[configurable(metadata(docs::examples = "k8s.container_image_id"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_image_id: OptionalTargetPath,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            pod_name: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_name")).into(),
            pod_namespace: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_namespace"))
                .into(),
            pod_uid: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_uid")).into(),
            pod_ip: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_ip")).into(),
            pod_ips: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_ips")).into(),
            pod_labels: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_labels"))
                .into(),
            pod_annotations: OwnedTargetPath::event(owned_value_path!(
                "kubernetes",
                "pod_annotations"
            ))
            .into(),
            pod_node_name: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_node_name"))
                .into(),
            pod_owner: OwnedTargetPath::event(owned_value_path!("kubernetes", "pod_owner")).into(),
            container_name: OwnedTargetPath::event(owned_value_path!(
                "kubernetes",
                "container_name"
            ))
            .into(),
            container_id: OwnedTargetPath::event(owned_value_path!("kubernetes", "container_id"))
                .into(),
            container_image: OwnedTargetPath::event(owned_value_path!(
                "kubernetes",
                "container_image"
            ))
            .into(),
            container_image_id: OwnedTargetPath::event(owned_value_path!(
                "kubernetes",
                "container_image_id"
            ))
            .into(),
        }
    }
}

/// Annotate the event with pod metadata.
pub struct PodMetadataAnnotator {
    pods_state_reader: Store<Pod>,
    fields_spec: FieldsSpec,
    log_namespace: LogNamespace,
}

impl PodMetadataAnnotator {
    /// Create a new [`PodMetadataAnnotator`].
    pub const fn new(
        pods_state_reader: Store<Pod>,
        fields_spec: FieldsSpec,
        log_namespace: LogNamespace,
    ) -> Self {
        Self {
            pods_state_reader,
            fields_spec,
            log_namespace,
        }
    }
}

impl PodMetadataAnnotator {
    /// Annotates an event with the information from the [`Pod::metadata`].
    pub fn annotate<'a>(&self, event: &mut Event, file: &'a str) -> Option<LogFileInfo<'a>> {
        let log = event.as_mut_log();
        let file_info = parse_log_file_path(file)?;
        let obj = ObjectRef::<Pod>::new(file_info.pod_name).within(file_info.pod_namespace);
        let resource = self.pods_state_reader.get(&obj)?;
        let pod: &Pod = resource.as_ref();

        annotate_from_file_info(log, &self.fields_spec, &file_info, self.log_namespace);
        annotate_from_metadata(log, &self.fields_spec, &pod.metadata, self.log_namespace);

        let container;
        if let Some(ref pod_spec) = pod.spec {
            annotate_from_pod_spec(log, &self.fields_spec, pod_spec, self.log_namespace);

            container = pod_spec
                .containers
                .iter()
                .find(|c| c.name == file_info.container_name);
            if let Some(container) = container {
                annotate_from_container(log, &self.fields_spec, container, self.log_namespace);
            }
        }

        if let Some(ref pod_status) = pod.status {
            annotate_from_pod_status(log, &self.fields_spec, pod_status, self.log_namespace);
            if let Some(ref container_statuses) = pod_status.container_statuses {
                let container_status = container_statuses
                    .iter()
                    .find(|c| c.name == file_info.container_name);
                if let Some(container_status) = container_status {
                    annotate_from_container_status(
                        log,
                        &self.fields_spec,
                        container_status,
                        self.log_namespace,
                    )
                }
            }
        }
        Some(file_info)
    }
}

fn annotate_from_file_info(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    file_info: &LogFileInfo<'_>,
    log_namespace: LogNamespace,
) {
    let legacy_key = fields_spec
        .container_name
        .path
        .as_ref()
        .map(|k| &k.path)
        .map(LegacyKey::Overwrite);

    log_namespace.insert_source_metadata(
        Config::NAME,
        log,
        legacy_key,
        path!("container_name"),
        file_info.container_name.to_owned(),
    );
}

fn annotate_from_metadata(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    metadata: &ObjectMeta,
    log_namespace: LogNamespace,
) {
    for (legacy_key, metadata_key, value) in [
        (&fields_spec.pod_name, path!("pod_name"), &metadata.name),
        (
            &fields_spec.pod_namespace,
            path!("pod_namespace"),
            &metadata.namespace,
        ),
        (&fields_spec.pod_uid, path!("pod_uid"), &metadata.uid),
    ]
    .iter()
    {
        if let Some(value) = value {
            let legacy_key = legacy_key
                .path
                .as_ref()
                .map(|k| &k.path)
                .map(LegacyKey::Overwrite);

            log_namespace.insert_source_metadata(
                Config::NAME,
                log,
                legacy_key,
                *metadata_key,
                value.to_owned(),
            );
        }
    }

    if let Some(owner_references) = &metadata.owner_references {
        let legacy_key = fields_spec
            .pod_owner
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("pod_owner"),
            format!("{}/{}", owner_references[0].kind, owner_references[0].name),
        )
    }

    if let Some(labels) = &metadata.labels {
        let legacy_key_prefix = fields_spec.pod_labels.path.as_ref().map(|k| &k.path);

        for (key, value) in labels.iter() {
            let key_path = path!(key);
            let legacy_key = legacy_key_prefix
                .map(|k| k.concat(key_path))
                .map(LegacyKey::Overwrite);

            log_namespace.insert_source_metadata(
                Config::NAME,
                log,
                legacy_key,
                path!("pod_labels", key),
                value.to_owned(),
            )
        }
    }

    if let Some(annotations) = &metadata.annotations {
        let legacy_key_prefix = fields_spec.pod_annotations.path.as_ref().map(|k| &k.path);

        for (key, value) in annotations.iter() {
            let key_path = path!(key);
            let legacy_key = legacy_key_prefix
                .map(|k| k.concat(key_path))
                .map(LegacyKey::Overwrite);

            log_namespace.insert_source_metadata(
                Config::NAME,
                log,
                legacy_key,
                path!("pod_annotations", key),
                value.to_owned(),
            )
        }
    }
}

fn annotate_from_pod_spec(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    pod_spec: &PodSpec,
    log_namespace: LogNamespace,
) {
    if let Some(value) = &pod_spec.node_name {
        let legacy_key = fields_spec
            .pod_node_name
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("pod_node_name"),
            value.to_owned(),
        )
    }
}

fn annotate_from_pod_status(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    pod_status: &PodStatus,
    log_namespace: LogNamespace,
) {
    if let Some(value) = &pod_status.pod_ip {
        let legacy_key = fields_spec
            .pod_ip
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("pod_ip"),
            value.to_owned(),
        )
    }

    if let Some(value) = &pod_status.pod_ips {
        let legacy_key = fields_spec
            .pod_ips
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        let value = value
            .iter()
            .filter_map(|k| k.ip.clone())
            .collect::<Vec<String>>();

        log_namespace.insert_source_metadata(Config::NAME, log, legacy_key, path!("pod_ips"), value)
    }
}

fn annotate_from_container_status(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    container_status: &ContainerStatus,
    log_namespace: LogNamespace,
) {
    if let Some(value) = &container_status.container_id {
        let legacy_key = fields_spec
            .container_id
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("container_id"),
            value.to_owned(),
        )
    }

    let legacy_key = fields_spec
        .container_image_id
        .path
        .as_ref()
        .map(|k| &k.path)
        .map(LegacyKey::Overwrite);

    log_namespace.insert_source_metadata(
        Config::NAME,
        log,
        legacy_key,
        path!("container_image_id"),
        container_status.image_id.to_owned(),
    )
}

fn annotate_from_container(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    container: &Container,
    log_namespace: LogNamespace,
) {
    if let Some(value) = &container.image {
        let legacy_key = fields_spec
            .container_image
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("container_image"),
            value.to_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use k8s_openapi::api::core::v1::PodIP;
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
                    annotations: Some(
                        vec![
                            ("sandbox0-annotation0".to_owned(), "val0".to_owned()),
                            ("sandbox0-annotation1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_name"),
                        "sandbox0-name",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_namespace"),
                        "sandbox0-ns",
                    );
                    log.insert(metadata_path!("kubernetes_logs", "pod_uid"), "sandbox0-uid");
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "sandbox0-label0"),
                        "val0",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "sandbox0-label1"),
                        "val1",
                    );
                    log.insert(
                        metadata_path!(
                            "kubernetes_logs",
                            "pod_annotations",
                            "sandbox0-annotation0"
                        ),
                        "val0",
                    );
                    log.insert(
                        metadata_path!(
                            "kubernetes_logs",
                            "pod_annotations",
                            "sandbox0-annotation1"
                        ),
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
                    annotations: Some(
                        vec![
                            ("sandbox0-annotation0".to_owned(), "val0".to_owned()),
                            ("sandbox0-annotation1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("kubernetes", "pod_name"), "sandbox0-name");
                    log.insert(event_path!("kubernetes", "pod_namespace"), "sandbox0-ns");
                    log.insert(event_path!("kubernetes", "pod_uid"), "sandbox0-uid");
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "sandbox0-label0"),
                        "val0",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "sandbox0-label1"),
                        "val1",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_annotations", "sandbox0-annotation0"),
                        "val0",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_annotations", "sandbox0-annotation1"),
                        "val1",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    pod_name: OwnedTargetPath::event(owned_value_path!("name")).into(),
                    pod_namespace: OwnedTargetPath::event(owned_value_path!("ns")).into(),
                    pod_uid: OwnedTargetPath::event(owned_value_path!("uid")).into(),
                    pod_labels: OwnedTargetPath::event(owned_value_path!("labels")).into(),
                    // ensure we can disable fields
                    pod_annotations: OptionalTargetPath::none(),
                    ..Default::default()
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
                    annotations: Some(
                        vec![
                            ("sandbox0-annotation0".to_owned(), "val0".to_owned()),
                            ("sandbox0-annotation1".to_owned(), "val1".to_owned()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("name"), "sandbox0-name");
                    log.insert(event_path!("ns"), "sandbox0-ns");
                    log.insert(event_path!("uid"), "sandbox0-uid");
                    log.insert(event_path!("labels", "sandbox0-label0"), "val0");
                    log.insert(event_path!("labels", "sandbox0-label1"), "val1");
                    log
                },
                LogNamespace::Legacy,
            ),
            // Ensure we properly handle labels with `.` as flat fields.
            (
                FieldsSpec::default(),
                ObjectMeta {
                    name: Some("sandbox0-name".to_owned()),
                    namespace: Some("sandbox0-ns".to_owned()),
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
                        metadata_path!("kubernetes_logs", "pod_name"),
                        "sandbox0-name",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_namespace"),
                        "sandbox0-ns",
                    );
                    log.insert(metadata_path!("kubernetes_logs", "pod_uid"), "sandbox0-uid");
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "nested0.label0"),
                        "val0",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "nested0.label1"),
                        "val1",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "nested1.label0"),
                        "val2",
                    );
                    log.insert(
                        metadata_path!("kubernetes_logs", "pod_labels", "nested2.label0.deep0"),
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
                    namespace: Some("sandbox0-ns".to_owned()),
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
                    log.insert(event_path!("kubernetes", "pod_name"), "sandbox0-name");
                    log.insert(event_path!("kubernetes", "pod_namespace"), "sandbox0-ns");
                    log.insert(event_path!("kubernetes", "pod_uid"), "sandbox0-uid");
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "nested0.label0"),
                        "val0",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "nested0.label1"),
                        "val1",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "nested1.label0"),
                        "val2",
                    );
                    log.insert(
                        event_path!("kubernetes", "pod_labels", "nested2.label0.deep0"),
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

    #[test]
    fn test_annotate_from_file_info() {
        let cases = vec![(
            FieldsSpec::default(),
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert(event_path!("kubernetes", "container_name"), "sandbox0-container0-name");
                log
            },
            LogNamespace::Legacy,
        ),(
            FieldsSpec{
                container_name: OwnedTargetPath::event(owned_value_path!("container_name")).into(),
                ..Default::default()
            },
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert(event_path!("container_name"), "sandbox0-container0-name");
                log
            },
            LogNamespace::Legacy,
        )];

        for (fields_spec, file, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            let file_info = parse_log_file_path(file).unwrap();
            annotate_from_file_info(&mut log, &fields_spec, &file_info, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_pod_spec() {
        let cases = vec![
            (
                FieldsSpec::default(),
                PodSpec::default(),
                LogEvent::default(),
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec::default(),
                PodSpec {
                    node_name: Some("sandbox0-node-name".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        event_path!("kubernetes", "pod_node_name"),
                        "sandbox0-node-name",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    pod_node_name: OwnedTargetPath::event(owned_value_path!("node_name")).into(),
                    ..Default::default()
                },
                PodSpec {
                    node_name: Some("sandbox0-node-name".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("node_name"), "sandbox0-node-name");
                    log
                },
                LogNamespace::Legacy,
            ),
        ];

        for (fields_spec, pod_spec, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_pod_spec(&mut log, &fields_spec, &pod_spec, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_pod_status() {
        let cases = vec![
            (
                FieldsSpec::default(),
                PodStatus::default(),
                LogEvent::default(),
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec::default(),
                PodStatus {
                    pod_ip: Some("192.168.1.2".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("kubernetes", "pod_ip"), "192.168.1.2");
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec::default(),
                PodStatus {
                    pod_ips: Some(vec![PodIP {
                        ip: Some("192.168.1.2".to_owned()),
                    }]),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    let ips_vec = vec!["192.168.1.2"];
                    log.insert(event_path!("kubernetes", "pod_ips"), ips_vec);
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    pod_ip: OwnedTargetPath::event(owned_value_path!(
                        "kubernetes",
                        "custom_pod_ip"
                    ))
                    .into(),
                    pod_ips: OwnedTargetPath::event(owned_value_path!(
                        "kubernetes",
                        "custom_pod_ips"
                    ))
                    .into(),
                    ..FieldsSpec::default()
                },
                PodStatus {
                    pod_ip: Some("192.168.1.2".to_owned()),
                    pod_ips: Some(vec![
                        PodIP {
                            ip: Some("192.168.1.2".to_owned()),
                        },
                        PodIP {
                            ip: Some("192.168.1.3".to_owned()),
                        },
                    ]),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("kubernetes", "custom_pod_ip"), "192.168.1.2");
                    let ips_vec = vec!["192.168.1.2", "192.168.1.3"];
                    log.insert(event_path!("kubernetes", "custom_pod_ips"), ips_vec);
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    pod_node_name: OwnedTargetPath::event(owned_value_path!("node_name")).into(),
                    ..FieldsSpec::default()
                },
                PodStatus {
                    pod_ip: Some("192.168.1.2".to_owned()),
                    pod_ips: Some(vec![
                        PodIP {
                            ip: Some("192.168.1.2".to_owned()),
                        },
                        PodIP {
                            ip: Some("192.168.1.3".to_owned()),
                        },
                    ]),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("kubernetes", "pod_ip"), "192.168.1.2");
                    let ips_vec = vec!["192.168.1.2", "192.168.1.3"];
                    log.insert(event_path!("kubernetes", "pod_ips"), ips_vec);
                    log
                },
                LogNamespace::Legacy,
            ),
        ];

        for (fields_spec, pod_status, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_pod_status(&mut log, &fields_spec, &pod_status, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_container_status() {
        let cases = vec![
            (
                FieldsSpec::default(),
                ContainerStatus::default(),
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("kubernetes", "container_image_id"), "");
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    ..FieldsSpec::default()
                },
                ContainerStatus {
                    container_id: Some("container_id_foo".to_owned()),
                    image_id: "test_image_id".to_owned(),
                    ..ContainerStatus::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        event_path!("kubernetes", "container_id"),
                        "container_id_foo",
                    );
                    log.insert(
                        event_path!("kubernetes", "container_image_id"),
                        "test_image_id",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
        ];
        for (fields_spec, container_status, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container_status(
                &mut log,
                &fields_spec,
                &container_status,
                log_namespace,
            );
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_suppress_annotation_fields() {
        let cases = vec![
            (
                FieldsSpec {
                    container_id: OptionalTargetPath::none(),
                    ..FieldsSpec::default()
                },
                ContainerStatus {
                    container_id: Some("container_id_foo".to_owned()),
                    image_id: "test_image_id".to_owned(),
                    ..ContainerStatus::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        event_path!("kubernetes", "container_image_id"),
                        "test_image_id",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    container_id: OptionalTargetPath::none(),
                    ..FieldsSpec::default()
                },
                ContainerStatus {
                    container_id: Some("container_id_foo".to_owned()),
                    image_id: "test_image_id".to_owned(),
                    ..ContainerStatus::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        metadata_path!("kubernetes_logs", "container_image_id"),
                        "test_image_id",
                    );
                    log
                },
                LogNamespace::Vector,
            ),
        ];
        for (fields_spec, container_status, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container_status(
                &mut log,
                &fields_spec,
                &container_status,
                log_namespace,
            );
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_container() {
        let cases = vec![
            (
                FieldsSpec::default(),
                Container::default(),
                LogEvent::default(),
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec::default(),
                Container {
                    image: Some("sandbox0-container-image".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        event_path!("kubernetes", "container_image"),
                        "sandbox0-container-image",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    container_image: OwnedTargetPath::event(owned_value_path!("container_image"))
                        .into(),
                    ..Default::default()
                },
                Container {
                    image: Some("sandbox0-container-image".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("container_image"), "sandbox0-container-image");
                    log
                },
                LogNamespace::Legacy,
            ),
        ];

        for (fields_spec, container, expected, log_namespace) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container(&mut log, &fields_spec, &container, log_namespace);
            assert_eq!(log, expected);
        }
    }
}
