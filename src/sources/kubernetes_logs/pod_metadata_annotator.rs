//! Annotates events with pod metadata.

#![deny(missing_docs)]

use k8s_openapi::{
    api::core::v1::{Container, ContainerStatus, Pod, PodSpec, PodStatus},
    apimachinery::pkg::{api::resource::Quantity, apis::meta::v1::ObjectMeta},
};
use kube::runtime::reflector::{ObjectRef, store::Store};
use vector_lib::{
    config::{LegacyKey, LogNamespace},
    configurable::configurable_component,
    lookup::{
        OwnedTargetPath,
        lookup_v2::{OptionalTargetPath, ValuePath},
        owned_value_path, path,
    },
};

use super::{
    Config,
    path_helpers::{LogFileInfo, parse_log_file_path},
    resource_quantity::parse_quantity,
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

    /// Event field for the Pod's `node_name`.
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

    /// Overrides the name of the log field used to add the container's CPU request
    /// in millicores to each event.
    ///
    /// Off by default, the field is only added to the event if this is set.
    #[configurable(metadata(docs::examples = ".k8s.container_cpu_request_millicores"))]
    #[configurable(metadata(docs::examples = "k8s.container_cpu_request_millicores"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_cpu_request_millicores: OptionalTargetPath,

    /// Overrides the name of the log field used to add the container's memory request
    /// in bytes to each event.
    ///
    /// Off by default, the field is only added to the event if this is set.
    #[configurable(metadata(docs::examples = ".k8s.container_memory_request_bytes"))]
    #[configurable(metadata(docs::examples = "k8s.container_memory_request_bytes"))]
    #[configurable(metadata(docs::examples = ""))]
    pub container_memory_request_bytes: OptionalTargetPath,

    /// Overrides the name of the log field used to add the Pod's effective CPU request
    /// in millicores to each event.
    ///
    /// Includes application containers, restartable init containers, peak initialization
    /// requests, and Pod overhead. An explicit Pod-level CPU request overrides the
    /// container aggregate before overhead is added. Uses desired requests from the Pod spec.
    /// Each quantity is rounded up to millicores before aggregation.
    ///
    /// Off by default in the legacy log namespace; always included in Vector metadata
    /// when available. Omitted if no CPU request is specified or a contributing quantity
    /// or total cannot be represented as a non-negative 64-bit integer.
    #[configurable(metadata(docs::examples = "k8s.pod_cpu_request_millicores"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_cpu_request_millicores: OptionalTargetPath,

    /// Overrides the name of the log field used to add the Pod's effective memory request
    /// in bytes to each event.
    ///
    /// Includes application containers, restartable init containers, peak initialization
    /// requests, and Pod overhead. An explicit Pod-level memory request overrides the
    /// container aggregate before overhead is added. Uses desired requests from the Pod spec.
    /// Each quantity is rounded up to bytes before aggregation.
    ///
    /// Off by default in the legacy log namespace; always included in Vector metadata
    /// when available. Omitted if no memory request is specified or a contributing quantity
    /// or total cannot be represented as a non-negative 64-bit integer.
    #[configurable(metadata(docs::examples = "k8s.pod_memory_request_bytes"))]
    #[configurable(metadata(docs::examples = ""))]
    pub pod_memory_request_bytes: OptionalTargetPath,

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
            container_cpu_request_millicores: OptionalTargetPath::none(),
            container_memory_request_bytes: OptionalTargetPath::none(),
            pod_cpu_request_millicores: OptionalTargetPath::none(),
            pod_memory_request_bytes: OptionalTargetPath::none(),
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
    ///
    /// When the pod is not found in the API store (e.g. already deleted), falls back
    /// to populating basic metadata (namespace, pod name, pod log directory ID, container name)
    /// from the log file path, which is always available. The path-derived ID is exposed as
    /// `pod_log_directory_id` rather than `pod_uid`, since for static pods it may be a config
    /// hash instead of the API Pod UID.
    pub fn annotate<'a>(&self, event: &mut Event, file: &'a str) -> Option<LogFileInfo<'a>> {
        let log = event.as_mut_log();
        let file_info = parse_log_file_path(file)?;
        let obj = ObjectRef::<Pod>::new(file_info.pod_name).within(file_info.pod_namespace);
        let resource = self.pods_state_reader.get(&obj);

        annotate_from_file_info(log, &self.fields_spec, &file_info, self.log_namespace);

        let Some(resource) = resource else {
            warn!(
                message = "Pod not found in API store, annotating from file path only.",
                pod_namespace = file_info.pod_namespace,
                pod_name = file_info.pod_name,
            );
            annotate_from_file_path(log, &self.fields_spec, &file_info, self.log_namespace);
            return Some(file_info);
        };
        let pod: &Pod = resource.as_ref();

        annotate_from_metadata(log, &self.fields_spec, &pod.metadata, self.log_namespace);

        let container;
        if let Some(ref pod_spec) = pod.spec {
            annotate_from_pod_spec(log, &self.fields_spec, pod_spec, self.log_namespace);

            container = pod_spec
                .containers
                .iter()
                .chain(pod_spec.init_containers.iter().flatten())
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
                    );
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

/// Fallback annotation from file path when pod is not in the API store.
///
/// Sets `pod_name` and `pod_namespace` from the parsed log file path, and exposes the path
/// directory ID as `pod_log_directory_id` (not `pod_uid`), since for static pods that segment
/// may be a config hash rather than the API Pod UID.
fn annotate_from_file_path(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    file_info: &LogFileInfo<'_>,
    log_namespace: LogNamespace,
) {
    for (field_spec, metadata_key, value) in [
        (&fields_spec.pod_name, path!("pod_name"), file_info.pod_name),
        (
            &fields_spec.pod_namespace,
            path!("pod_namespace"),
            file_info.pod_namespace,
        ),
    ] {
        let legacy_key = field_spec
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            metadata_key,
            value.to_owned(),
        );
    }

    let pod_log_directory_id_path = owned_value_path!("kubernetes", "pod_log_directory_id");
    log_namespace.insert_source_metadata(
        Config::NAME,
        log,
        Some(LegacyKey::Overwrite(&pod_log_directory_id_path)),
        path!("pod_log_directory_id"),
        file_info.pod_uid.to_owned(),
    );
}

fn annotate_from_metadata(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    metadata: &ObjectMeta,
    log_namespace: LogNamespace,
) {
    for (legacy_key, metadata_key, value) in &[
        (&fields_spec.pod_name, path!("pod_name"), &metadata.name),
        (
            &fields_spec.pod_namespace,
            path!("pod_namespace"),
            &metadata.namespace,
        ),
        (&fields_spec.pod_uid, path!("pod_uid"), &metadata.uid),
    ] {
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
        );
    }

    if let Some(labels) = &metadata.labels {
        let legacy_key_prefix = fields_spec.pod_labels.path.as_ref().map(|k| &k.path);

        for (key, value) in labels {
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
            );
        }
    }

    if let Some(annotations) = &metadata.annotations {
        let legacy_key_prefix = fields_spec.pod_annotations.path.as_ref().map(|k| &k.path);

        for (key, value) in annotations {
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
            );
        }
    }
}

fn annotate_from_pod_spec(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    pod_spec: &PodSpec,
    log_namespace: LogNamespace,
) {
    for (resource, decimal_scale, field_name, field) in [
        (
            "cpu",
            3,
            "pod_cpu_request_millicores",
            &fields_spec.pod_cpu_request_millicores,
        ),
        (
            "memory",
            0,
            "pod_memory_request_bytes",
            &fields_spec.pod_memory_request_bytes,
        ),
    ] {
        let legacy_key = field
            .path
            .as_ref()
            .map(|path| LegacyKey::Overwrite(&path.path));
        if log_namespace == LogNamespace::Legacy && legacy_key.is_none() {
            continue;
        }
        if let Some(value) = pod_resource_request(pod_spec, resource, decimal_scale) {
            log_namespace.insert_source_metadata(
                Config::NAME,
                log,
                legacy_key,
                path!(field_name),
                value,
            );
        }
    }

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
        );
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
        );
    }

    if let Some(value) = &pod_status.pod_ips {
        let legacy_key = fields_spec
            .pod_ips
            .path
            .as_ref()
            .map(|k| &k.path)
            .map(LegacyKey::Overwrite);

        let value = value.iter().map(|k| k.ip.clone()).collect::<Vec<String>>();

        log_namespace.insert_source_metadata(
            Config::NAME,
            log,
            legacy_key,
            path!("pod_ips"),
            value,
        );
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
        );
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
        container_status.image_id.clone(),
    );
}

// Mirror Kubernetes' spec-based PodRequests calculation, normalizing quantities to the
// same integer units as the container fields. Missing requests contribute zero, but
// leave the field absent if none are specified. Invalid quantities invalidate the total.
fn pod_resource_request(pod_spec: &PodSpec, resource: &str, decimal_scale: i32) -> Option<i64> {
    let mut specified = false;
    let mut parse = |quantity: Option<&Quantity>| {
        quantity.map_or(Some(0), |quantity| {
            specified = true;
            parse_quantity(&quantity.0, decimal_scale)
        })
    };
    let mut total = if let Some(quantity) = pod_spec
        .resources
        .as_ref()
        .and_then(|resources| resources.requests.as_ref())
        .and_then(|requests| requests.get(resource))
    {
        parse(Some(quantity))?
    } else {
        let mut running = 0_i64;
        for container in &pod_spec.containers {
            running = running.checked_add(parse(container_request(container, resource))?)?;
        }

        let mut sidecars = 0_i64;
        let mut initialization = 0;
        for container in pod_spec.init_containers.iter().flatten() {
            let request = parse(container_request(container, resource))?;
            let stage = if container.restart_policy.as_deref() == Some("Always") {
                running = running.checked_add(request)?;
                sidecars = sidecars.checked_add(request)?;
                sidecars
            } else {
                // Only preceding restartable init containers run alongside this init.
                sidecars.checked_add(request)?
            };
            initialization = initialization.max(stage);
        }
        running.max(initialization)
    };
    total = total.checked_add(parse(
        pod_spec
            .overhead
            .as_ref()
            .and_then(|overhead| overhead.get(resource)),
    )?)?;
    specified.then_some(total)
}

fn container_request<'a>(container: &'a Container, resource: &str) -> Option<&'a Quantity> {
    container
        .resources
        .as_ref()?
        .requests
        .as_ref()?
        .get(resource)
}

fn annotate_from_container(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    container: &Container,
    log_namespace: LogNamespace,
) {
    if let Some(requests) = container
        .resources
        .as_ref()
        .and_then(|resources| resources.requests.as_ref())
    {
        for (resource, decimal_scale, field_name, field) in [
            (
                "cpu",
                3,
                "container_cpu_request_millicores",
                &fields_spec.container_cpu_request_millicores,
            ),
            (
                "memory",
                0,
                "container_memory_request_bytes",
                &fields_spec.container_memory_request_bytes,
            ),
        ] {
            let legacy_key = field
                .path
                .as_ref()
                .map(|path| LegacyKey::Overwrite(&path.path));
            if log_namespace == LogNamespace::Legacy && legacy_key.is_none() {
                continue;
            }
            if let Some(value) = requests
                .get(resource)
                .and_then(|quantity| parse_quantity(&quantity.0, decimal_scale))
            {
                log_namespace.insert_source_metadata(
                    Config::NAME,
                    log,
                    legacy_key,
                    path!(field_name),
                    value,
                );
            }
        }
    }

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
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use k8s_openapi::api::core::v1::PodIP;
    use kube::runtime::{reflector::store::Writer, watcher};
    use serde_json::json;
    use similar_asserts::assert_eq;
    use vector_lib::lookup::{event_path, lookup_v2::parse_target_path, metadata_path};

    use super::*;
    use crate::event::Value;

    fn resource_path(namespace: LogNamespace, field: &str) -> OwnedTargetPath {
        match namespace {
            LogNamespace::Legacy => OwnedTargetPath::event(owned_value_path!("kubernetes", field)),
            LogNamespace::Vector => {
                OwnedTargetPath::metadata(owned_value_path!("kubernetes_logs", field))
            }
        }
    }

    fn resource_request_fields() -> FieldsSpec {
        serde_json::from_value(json!({
            "container_cpu_request_millicores": "kubernetes.container_cpu_request_millicores",
            "container_memory_request_bytes": "kubernetes.container_memory_request_bytes",
            "pod_cpu_request_millicores": "kubernetes.pod_cpu_request_millicores",
            "pod_memory_request_bytes": "kubernetes.pod_memory_request_bytes",
        }))
        .unwrap()
    }

    fn pod_with_resource_requests() -> Pod {
        serde_json::from_value(json!({
            "metadata": {"name": "pod", "namespace": "namespace", "uid": "uid"},
            "spec": {
                "containers": [
                    {
                        "name": "app",
                        "resources": {"requests": {"cpu": "500m", "memory": "256Mi"}}
                    },
                    {
                        "name": "sidecar",
                        "resources": {"requests": {"cpu": "100m", "memory": "32Mi"}}
                    }
                ],
                "initContainers": [{
                    "name": "init",
                    "resources": {"requests": {"cpu": "1", "memory": "64Mi"}}
                }]
            }
        }))
        .unwrap()
    }

    #[test]
    fn annotates_logging_container_resource_requests() {
        let mut writer = Writer::default();
        writer.apply_watcher_event(&watcher::Event::Apply(pod_with_resource_requests()));

        let namespace = LogNamespace::Legacy;
        let annotator =
            PodMetadataAnnotator::new(writer.as_reader(), resource_request_fields(), namespace);
        for (container, cpu, memory) in [
            ("app", Some(500), Some(268_435_456)),
            ("sidecar", Some(100), Some(33_554_432)),
            ("init", Some(1000), Some(67_108_864)),
            ("unknown", None, None),
        ] {
            let mut event = Event::Log(LogEvent::default());
            annotator
                .annotate(
                    &mut event,
                    &format!("/var/log/pods/namespace_pod_uid/{container}/0.log"),
                )
                .unwrap();
            let log = event.as_log();
            assert_eq!(
                log.get(&resource_path(
                    namespace,
                    "container_cpu_request_millicores"
                )),
                cpu.map(Value::from).as_ref(),
                "{container}: CPU",
            );
            assert_eq!(
                log.get(&resource_path(namespace, "container_memory_request_bytes")),
                memory.map(Value::from).as_ref(),
                "{container}: memory",
            );
        }
    }

    #[test]
    fn pod_requests_follow_cached_pod_updates_for_every_container() {
        let mut writer = Writer::default();
        let mut pod = pod_with_resource_requests();
        let mut resized = pod.clone();
        resized.spec.as_mut().unwrap().containers[0]
            .resources
            .as_mut()
            .unwrap()
            .requests
            .as_mut()
            .unwrap()
            .insert("cpu".into(), Quantity("2".into()));
        let mut replacement = pod.clone();
        replacement.metadata.uid = Some("replacement".into());
        replacement.spec.as_mut().unwrap().resources = Some(
            serde_json::from_value(json!({
                "requests": {"cpu": "3"}
            }))
            .unwrap(),
        );

        for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
            let annotator =
                PodMetadataAnnotator::new(writer.as_reader(), resource_request_fields(), namespace);
            for (event, cpu, memory) in [
                (watcher::Event::Delete(pod.clone()), None, None),
                (
                    watcher::Event::Apply(pod.clone()),
                    Some(1000),
                    Some(301_989_888),
                ),
                (
                    watcher::Event::Apply(resized.clone()),
                    Some(2100),
                    Some(301_989_888),
                ),
                (
                    watcher::Event::Apply(replacement.clone()),
                    Some(3000),
                    Some(301_989_888),
                ),
                (watcher::Event::Delete(replacement.clone()), None, None),
            ] {
                writer.apply_watcher_event(&event);
                for container in ["app", "sidecar", "init", "unknown"] {
                    let mut log = Event::Log(LogEvent::default());
                    annotator
                        .annotate(
                            &mut log,
                            &format!("/var/log/pods/namespace_pod_uid/{container}/0.log"),
                        )
                        .unwrap();
                    assert_eq!(
                        log.as_log()
                            .get(&resource_path(namespace, "pod_cpu_request_millicores")),
                        cpu.map(Value::from).as_ref()
                    );
                    assert_eq!(
                        log.as_log()
                            .get(&resource_path(namespace, "pod_memory_request_bytes")),
                        memory.map(Value::from).as_ref()
                    );
                }
            }
        }

        // A relist replaces the cached spec just like an ordinary watch update.
        writer.apply_watcher_event(&watcher::Event::Init);
        pod.spec.as_mut().unwrap().containers.clear();
        writer.apply_watcher_event(&watcher::Event::InitApply(pod));
        writer.apply_watcher_event(&watcher::Event::InitDone);
        let annotator = PodMetadataAnnotator::new(
            writer.as_reader(),
            resource_request_fields(),
            LogNamespace::Legacy,
        );
        let mut log = Event::Log(LogEvent::default());
        annotator
            .annotate(&mut log, "/var/log/pods/namespace_pod_uid/init/0.log")
            .unwrap();
        assert_eq!(
            log.as_log()
                .get(event_path!("kubernetes", "pod_cpu_request_millicores")),
            Some(&Value::from(1000))
        );
    }

    #[test]
    fn pod_requests_use_effective_spec_requests() {
        let container = |cpu: &str, memory: &str| {
            json!({
                "name": "container", "resources": {"requests": {"cpu": cpu, "memory": memory}}
            })
        };
        let sidecar = |cpu: &str, memory: &str| {
            let mut value = container(cpu, memory);
            value["restartPolicy"] = json!("Always");
            value
        };
        for (spec, cpu, memory) in [
            (json!({"containers": []}), None, None),
            (
                json!({"containers": [{"name": "app", "resources": {"limits": {"cpu": "1"}}}]}),
                None,
                None,
            ),
            (
                json!({"containers": [container("0", "0")]}),
                Some(0),
                Some(0),
            ),
            (
                json!({"containers": [container("500m", "128Mi"), container("100m", "32Mi"), {"name": "no-requests"}]}),
                Some(600),
                Some(167_772_160),
            ),
            (
                json!({"containers": [container("500m", "128Mi")], "initContainers": [container("1", "32Mi"), container("250m", "256Mi")]}),
                Some(1000),
                Some(268_435_456),
            ),
            (
                json!({"containers": [container("500m", "128Mi")], "initContainers": [sidecar("100m", "32Mi"), container("1", "256Mi"), sidecar("200m", "16Mi")]}),
                Some(1100),
                Some(301_989_888),
            ),
            (
                json!({"containers": [container("500m", "128Mi")], "initContainers": [container("1", "256Mi"), sidecar("100m", "32Mi"), sidecar("200m", "16Mi")]}),
                Some(1000),
                Some(268_435_456),
            ),
            (
                json!({"containers": [container("2", "512Mi")], "initContainers": [sidecar("100m", "32Mi"), container("1", "256Mi"), sidecar("200m", "16Mi")]}),
                Some(2300),
                Some(587_202_560),
            ),
            (
                json!({"containers": [container("500m", "128Mi")], "resources": {"requests": {"cpu": "2"}}, "overhead": {"cpu": "50m", "memory": "16Mi"}}),
                Some(2050),
                Some(150_994_944),
            ),
            (
                json!({"containers": [], "overhead": {"cpu": "50m"}}),
                Some(50),
                None,
            ),
        ] {
            let spec: PodSpec = serde_json::from_value(spec).unwrap();
            assert_eq!(pod_resource_request(&spec, "cpu", 3), cpu, "CPU: {spec:?}");
            assert_eq!(
                pod_resource_request(&spec, "memory", 0),
                memory,
                "memory: {spec:?}"
            );
        }
    }

    #[test]
    fn pod_requests_handle_invalid_and_unrepresentable_quantities() {
        let container = |cpu: &str, memory: &str| {
            json!({
                "name": "container", "resources": {"requests": {"cpu": cpu, "memory": memory}}
            })
        };
        for (spec, cpu, memory) in [
            (
                json!({"containers": [container("invalid", "128Mi")]}),
                None,
                Some(134_217_728),
            ),
            (
                json!({"containers": [container("-1", "128Mi")]}),
                None,
                Some(134_217_728),
            ),
            (
                json!({"containers": [container("1", "1e19")]}),
                Some(1000),
                None,
            ),
            (
                json!({"containers": [container("1", "5e18"), container("1", "5e18")]}),
                Some(2000),
                None,
            ),
            (
                json!({"containers": [container("1", "5e18")], "overhead": {"memory": "5e18"}}),
                Some(1000),
                None,
            ),
            (
                json!({"containers": [container("1", "128Mi")], "overhead": {"cpu": "invalid"}}),
                None,
                Some(134_217_728),
            ),
            (
                json!({"containers": [container("invalid", "128Mi")], "resources": {"requests": {"cpu": "2"}}}),
                Some(2000),
                Some(134_217_728),
            ),
            (
                json!({"containers": [container("1", "128Mi")], "resources": {"requests": {"cpu": "invalid"}}}),
                None,
                Some(134_217_728),
            ),
            (
                json!({"containers": [container("100u", "400m"), container("100u", "400m")]}),
                Some(2),
                Some(2),
            ),
        ] {
            let spec: PodSpec = serde_json::from_value(spec).unwrap();
            assert_eq!(pod_resource_request(&spec, "cpu", 3), cpu, "CPU: {spec:?}");
            assert_eq!(
                pod_resource_request(&spec, "memory", 0),
                memory,
                "memory: {spec:?}"
            );
        }
    }

    #[test]
    fn pod_request_fields_follow_log_namespace() {
        let mut spec = pod_with_resource_requests().spec.unwrap();
        spec.node_name = None;
        for (cpu_field, memory_field) in [
            (None, None),
            (Some(""), Some("")),
            (Some("custom.cpu"), None),
            (None, Some("custom.memory")),
            (Some("custom.cpu"), Some("custom.memory")),
        ] {
            let mut config = json!({});
            if let Some(field) = cpu_field {
                config["pod_cpu_request_millicores"] = json!(field);
            }
            if let Some(field) = memory_field {
                config["pod_memory_request_bytes"] = json!(field);
            }
            let fields: FieldsSpec = serde_json::from_value(config).unwrap();
            for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
                let mut actual = LogEvent::default();
                annotate_from_pod_spec(&mut actual, &fields, &spec, namespace);
                let mut expected = LogEvent::default();
                for (field, name, value) in [
                    (cpu_field, "pod_cpu_request_millicores", 1000),
                    (memory_field, "pod_memory_request_bytes", 301_989_888),
                ] {
                    if namespace == LogNamespace::Vector {
                        expected.insert(&resource_path(namespace, name), value);
                    } else if let Some(field) = field.filter(|field| !field.is_empty()) {
                        expected.insert(&parse_target_path(field).unwrap(), value);
                    }
                }
                assert_eq!(actual, expected);
            }
        }
    }

    #[test]
    fn resource_requests_convert_values() {
        let namespace = LogNamespace::Legacy;
        for (resources, cpu, memory) in [
            (json!(null), None, None),
            (json!({}), None, None),
            (json!({"limits": {"cpu": "1", "memory": "1Gi"}}), None, None),
            (json!({"requests": {"cpu": "250m"}}), Some(250), None),
            (
                json!({"requests": {"cpu": "1.5", "memory": "1.5Gi"}}),
                Some(1500),
                Some(1_610_612_736),
            ),
            (
                json!({"requests": {"cpu": "100u", "memory": "400m"}}),
                Some(1),
                Some(1),
            ),
            (
                json!({"requests": {"cpu": "-1", "memory": "-1Mi"}}),
                None,
                None,
            ),
            (json!({"requests": {"memory": "1M"}}), None, Some(1_000_000)),
            (
                json!({"requests": {"cpu": "0", "memory": "0"}}),
                Some(0),
                Some(0),
            ),
            (
                json!({"requests": {"cpu": "invalid", "memory": "1Mi"}}),
                None,
                Some(1_048_576),
            ),
            (
                json!({"requests": {"cpu": "1", "memory": "1e19"}}),
                Some(1000),
                None,
            ),
        ] {
            let container: Container = serde_json::from_value(json!({
                "name": "app",
                "resources": resources,
            }))
            .unwrap();
            let mut log = LogEvent::default();
            annotate_from_container(&mut log, &resource_request_fields(), &container, namespace);
            assert_eq!(
                log.get(&resource_path(
                    namespace,
                    "container_cpu_request_millicores"
                )),
                cpu.map(Value::from).as_ref(),
            );
            assert_eq!(
                log.get(&resource_path(namespace, "container_memory_request_bytes")),
                memory.map(Value::from).as_ref(),
            );
        }
    }

    #[test]
    fn resource_request_fields_follow_log_namespace() {
        let pod = pod_with_resource_requests();
        let container = &pod.spec.as_ref().unwrap().containers[0];
        for (name, cpu_field, memory_field) in [
            ("omitted", None, None),
            ("both empty", Some(""), Some("")),
            ("CPU only", Some("custom.cpu"), None),
            ("memory only", None, Some("custom.memory")),
            ("both enabled", Some("custom.cpu"), Some("custom.memory")),
        ] {
            let mut config = json!({});
            if let Some(path) = cpu_field {
                config["container_cpu_request_millicores"] = json!(path);
            }
            if let Some(path) = memory_field {
                config["container_memory_request_bytes"] = json!(path);
            }
            let fields: FieldsSpec = serde_json::from_value(config).unwrap();
            for namespace in [LogNamespace::Legacy, LogNamespace::Vector] {
                let mut log = LogEvent::default();
                annotate_from_container(&mut log, &fields, container, namespace);
                let mut expected = LogEvent::default();
                if namespace == LogNamespace::Vector || cpu_field == Some("custom.cpu") {
                    let path = match namespace {
                        LogNamespace::Legacy => {
                            OwnedTargetPath::event(owned_value_path!("custom", "cpu"))
                        }
                        LogNamespace::Vector => {
                            resource_path(namespace, "container_cpu_request_millicores")
                        }
                    };
                    expected.insert(&path, 500);
                }
                if namespace == LogNamespace::Vector || memory_field == Some("custom.memory") {
                    let path = match namespace {
                        LogNamespace::Legacy => {
                            OwnedTargetPath::event(owned_value_path!("custom", "memory"))
                        }
                        LogNamespace::Vector => {
                            resource_path(namespace, "container_memory_request_bytes")
                        }
                    };
                    expected.insert(&path, 268_435_456);
                }
                assert_eq!(log, expected, "{name}, {namespace:?}");
            }
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Splitting this fixture is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
    )]
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

        for (fields_spec, metadata, expected, log_namespace) in cases {
            let mut log = LogEvent::default();
            annotate_from_metadata(&mut log, &fields_spec, &metadata, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_file_info() {
        let path = &format!(
            "{}{}",
            std::path::MAIN_SEPARATOR,
            [
                "var",
                "log",
                "pods",
                "sandbox0-ns_sandbox0-name_sandbox0-uid",
                "sandbox0-container0-name",
                "1.log",
            ]
            .iter()
            .collect::<PathBuf>()
            .into_os_string()
            .into_string()
            .unwrap()
        );
        let s_path = path.as_str();
        let cases = vec![
            (
                FieldsSpec::default(),
                s_path,
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        event_path!("kubernetes", "container_name"),
                        "sandbox0-container0-name",
                    );
                    log
                },
                LogNamespace::Legacy,
            ),
            (
                FieldsSpec {
                    container_name: OwnedTargetPath::event(owned_value_path!("container_name"))
                        .into(),
                    ..Default::default()
                },
                s_path,
                {
                    let mut log = LogEvent::default();
                    log.insert(event_path!("container_name"), "sandbox0-container0-name");
                    log
                },
                LogNamespace::Legacy,
            ),
        ];

        for (fields_spec, file, expected, log_namespace) in cases {
            let mut log = LogEvent::default();
            let file_info = parse_log_file_path(file).unwrap();
            annotate_from_file_info(&mut log, &fields_spec, &file_info, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_file_path() {
        let path = &format!(
            "{}{}",
            std::path::MAIN_SEPARATOR,
            [
                "var",
                "log",
                "pods",
                "sandbox0-ns_sandbox0-name_sandbox0-uid",
                "sandbox0-container0-name",
                "1.log",
            ]
            .iter()
            .collect::<PathBuf>()
            .into_os_string()
            .into_string()
            .unwrap()
        );
        let fields_spec = FieldsSpec::default();
        let file_info = parse_log_file_path(path).unwrap();

        let mut log = LogEvent::default();
        annotate_from_file_path(&mut log, &fields_spec, &file_info, LogNamespace::Legacy);

        let mut expected = LogEvent::default();
        expected.insert(event_path!("kubernetes", "pod_name"), "sandbox0-name");
        expected.insert(event_path!("kubernetes", "pod_namespace"), "sandbox0-ns");
        expected.insert(
            event_path!("kubernetes", "pod_log_directory_id"),
            "sandbox0-uid",
        );

        assert_eq!(log, expected);
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

        for (fields_spec, pod_spec, expected, log_namespace) in cases {
            let mut log = LogEvent::default();
            annotate_from_pod_spec(&mut log, &fields_spec, &pod_spec, log_namespace);
            assert_eq!(log, expected);
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "Splitting this fixture is deferred to keep the lint cleanup mechanical. https://github.com/vectordotdev/vector/issues/23659"
    )]
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
                        ip: "192.168.1.2".to_owned(),
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
                            ip: "192.168.1.2".to_owned(),
                        },
                        PodIP {
                            ip: "192.168.1.3".to_owned(),
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
                            ip: "192.168.1.2".to_owned(),
                        },
                        PodIP {
                            ip: "192.168.1.3".to_owned(),
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

        for (fields_spec, pod_status, expected, log_namespace) in cases {
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
        for (fields_spec, container_status, expected, log_namespace) in cases {
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

        for (fields_spec, container, expected, log_namespace) in cases {
            let mut log = LogEvent::default();
            annotate_from_container(&mut log, &fields_spec, &container, log_namespace);
            assert_eq!(log, expected);
        }
    }
}
