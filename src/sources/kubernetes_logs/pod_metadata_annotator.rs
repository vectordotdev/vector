//! Annotates events with pod metadata.

#![deny(missing_docs)]

use super::path_helpers::{parse_log_file_path, LogFileInfo};
use crate::{
    event::{LogEvent, LookupBuf},
    kubernetes as k8s, Event,
};
use evmap::ReadHandle;
use k8s_openapi::{
    api::core::v1::{Container, ContainerStatus, Pod, PodSpec, PodStatus},
    apimachinery::pkg::apis::meta::v1::ObjectMeta,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct FieldsSpec {
    pub pod_name: LookupBuf,
    pub pod_namespace: LookupBuf,
    pub pod_uid: LookupBuf,
    pub pod_ip: LookupBuf,
    pub pod_ips: LookupBuf,
    pub pod_labels: LookupBuf,
    pub pod_node_name: LookupBuf,
    pub container_name: LookupBuf,
    pub container_id: LookupBuf,
    pub container_image: LookupBuf,
}

impl Default for FieldsSpec {
    fn default() -> Self {
        Self {
            pod_name: LookupBuf::from_str("kubernetes.pod_name").unwrap(),
            pod_namespace: LookupBuf::from_str("kubernetes.pod_namespace").unwrap(),
            pod_uid: LookupBuf::from_str("kubernetes.pod_uid").unwrap(),
            pod_ip: LookupBuf::from_str("kubernetes.pod_ip").unwrap(),
            pod_ips: LookupBuf::from_str("kubernetes.pod_ips").unwrap(),
            pod_labels: LookupBuf::from_str("kubernetes.pod_labels").unwrap(),
            pod_node_name: LookupBuf::from_str("kubernetes.pod_node_name").unwrap(),
            container_name: LookupBuf::from_str("kubernetes.container_name").unwrap(),
            container_id: LookupBuf::from_str("kubernetes.container_id").unwrap(),
            container_image: LookupBuf::from_str("kubernetes.container_image").unwrap(),
        }
    }
}

/// Annotate the event with pod metadata.
pub struct PodMetadataAnnotator {
    pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
    fields_spec: FieldsSpec,
}

impl PodMetadataAnnotator {
    /// Create a new [`PodMetadataAnnotator`].
    pub fn new(
        pods_state_reader: ReadHandle<String, k8s::state::evmap::Value<Pod>>,
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
    pub fn annotate<'a>(&self, event: &mut Event, file: &'a str) -> Option<LogFileInfo<'a>> {
        let log = event.as_mut_log();
        let file_info = parse_log_file_path(file)?;
        let guard = self.pods_state_reader.get(file_info.pod_uid)?;
        let entry = guard.get_one()?;
        let pod: &Pod = entry.as_ref();

        annotate_from_file_info(log, &self.fields_spec, &file_info);
        annotate_from_metadata(log, &self.fields_spec, &pod.metadata);

        let container;
        if let Some(ref pod_spec) = pod.spec {
            annotate_from_pod_spec(log, &self.fields_spec, pod_spec);

            container = pod_spec
                .containers
                .iter()
                .find(|c| c.name == file_info.container_name);
            if let Some(container) = container {
                annotate_from_container(log, &self.fields_spec, container);
            }
        }

        if let Some(ref pod_status) = pod.status {
            annotate_from_pod_status(log, &self.fields_spec, pod_status);
            if let Some(ref container_statuses) = pod_status.container_statuses {
                let container_status = container_statuses
                    .iter()
                    .find(|c| c.name == file_info.container_name);
                if let Some(container_status) = container_status {
                    annotate_from_container_status(log, &self.fields_spec, container_status)
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
) {
    log.insert(
        fields_spec.container_name.clone(),
        file_info.container_name.to_owned(),
    );
}

fn annotate_from_metadata(log: &mut LogEvent, fields_spec: &FieldsSpec, metadata: &ObjectMeta) {
    for (ref key, ref val) in [
        (&fields_spec.pod_name, &metadata.name),
        (&fields_spec.pod_namespace, &metadata.namespace),
        (&fields_spec.pod_uid, &metadata.uid),
    ]
    .iter()
    {
        if let Some(ref val) = val {
            log.insert((*key).clone(), val.to_owned());
        }
    }

    if let Some(labels) = &metadata.labels {
        // Calculate and cache the prefix path.
        let prefix_path = &fields_spec.pod_labels;
        for (key, val) in labels.iter() {
            let mut path = prefix_path.clone();
            path.push_back(key.to_string());
            log.insert(path, val.to_owned());
        }
    }
}

fn annotate_from_pod_spec(log: &mut LogEvent, fields_spec: &FieldsSpec, pod_spec: &PodSpec) {
    for (ref key, ref val) in [(&fields_spec.pod_node_name, &pod_spec.node_name)].iter() {
        if let Some(ref val) = val {
            log.insert((*key).clone(), val.to_owned());
        }
    }
}

fn annotate_from_pod_status(log: &mut LogEvent, fields_spec: &FieldsSpec, pod_status: &PodStatus) {
    for (ref key, ref val) in [(&fields_spec.pod_ip, &pod_status.pod_ip)].iter() {
        if let Some(ref val) = val {
            log.insert((*key).to_owned(), val.to_owned());
        }
    }

    for (ref key, ref val) in [(&fields_spec.pod_ips, &pod_status.pod_ips)].iter() {
        if let Some(ref val) = val {
            let inner: Vec<String> = val
                .iter()
                .filter_map(|v| v.ip.clone())
                .collect::<Vec<String>>();
            log.insert((*key).clone(), inner);
        }
    }
}

fn annotate_from_container_status(
    log: &mut LogEvent,
    fields_spec: &FieldsSpec,
    container_status: &ContainerStatus,
) {
    for (ref key, ref val) in [(&fields_spec.container_id, &container_status.container_id)].iter() {
        if let Some(val) = val {
            log.insert((*key).clone(), val.to_owned());
        }
    }
}

fn annotate_from_container(log: &mut LogEvent, fields_spec: &FieldsSpec, container: &Container) {
    for (ref key, ref val) in [(&fields_spec.container_image, &container.image)].iter() {
        if let Some(val) = val {
            log.insert((*key).clone(), val.to_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use k8s_openapi::api::core::v1::PodIP;

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
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_name").unwrap(),
                        "sandbox0-name",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_namespace").unwrap(),
                        "sandbox0-ns",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_uid").unwrap(),
                        "sandbox0-uid",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.sandbox0-label0").unwrap(),
                        "val0",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.sandbox0-label1").unwrap(),
                        "val1",
                    );
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_name: LookupBuf::from("name"),
                    pod_namespace: LookupBuf::from("ns"),
                    pod_uid: LookupBuf::from("uid"),
                    pod_labels: LookupBuf::from("labels"),
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
                    ..ObjectMeta::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("name", "sandbox0-name");
                    log.insert("ns", "sandbox0-ns");
                    log.insert("uid", "sandbox0-uid");
                    log.insert(
                        LookupBuf::from_str("labels.sandbox0-label0").unwrap(),
                        "val0",
                    );
                    log.insert(
                        LookupBuf::from_str("labels.sandbox0-label1").unwrap(),
                        "val1",
                    );
                    log
                },
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
                        LookupBuf::from_str("kubernetes.pod_name").unwrap(),
                        "sandbox0-name",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_namespace").unwrap(),
                        "sandbox0-ns",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_uid").unwrap(),
                        "sandbox0-uid",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.\"nested0.label0\"").unwrap(),
                        "val0",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.\"nested0.label1\"").unwrap(),
                        "val1",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.\"nested1.label0\"").unwrap(),
                        "val2",
                    );
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_labels.\"nested2.label0.deep0\"")
                            .unwrap(),
                        "val3",
                    );
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

    #[test]
    fn test_annotate_from_file_info() {
        let cases = vec![(
            FieldsSpec::default(),
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert(LookupBuf::from_str("kubernetes.container_name").unwrap(), "sandbox0-container0-name");
                log
            },
        ),(
            FieldsSpec{
                container_name: LookupBuf::from("container_name"),
                ..Default::default()
            },
            "/var/log/pods/sandbox0-ns_sandbox0-name_sandbox0-uid/sandbox0-container0-name/1.log",
            {
                let mut log = LogEvent::default();
                log.insert("container_name", "sandbox0-container0-name");
                log
            },
        )];

        for (fields_spec, file, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            let file_info = parse_log_file_path(file).unwrap();
            annotate_from_file_info(&mut log, &fields_spec, &file_info);
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
                        LookupBuf::from_str("kubernetes.pod_node_name").unwrap(),
                        "sandbox0-node-name",
                    );
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_node_name: LookupBuf::from("node_name"),
                    ..Default::default()
                },
                PodSpec {
                    node_name: Some("sandbox0-node-name".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert("node_name", "sandbox0-node-name");
                    log
                },
            ),
        ];

        for (fields_spec, pod_spec, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_pod_spec(&mut log, &fields_spec, &pod_spec);
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
            ),
            (
                FieldsSpec::default(),
                PodStatus {
                    pod_ip: Some("192.168.1.2".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_ip").unwrap(),
                        "192.168.1.2",
                    );
                    log
                },
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
                    log.insert(LookupBuf::from_str("kubernetes.pod_ips").unwrap(), ips_vec);
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_ip: LookupBuf::from_str("kubernetes.custom_pod_ip").unwrap(),
                    pod_ips: LookupBuf::from_str("kubernetes.custom_pod_ips").unwrap(),
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
                    log.insert(
                        LookupBuf::from_str("kubernetes.custom_pod_ip").unwrap(),
                        "192.168.1.2",
                    );
                    let ips_vec = vec!["192.168.1.2", "192.168.1.3"];
                    log.insert(
                        LookupBuf::from_str("kubernetes.custom_pod_ips").unwrap(),
                        ips_vec,
                    );
                    log
                },
            ),
            (
                FieldsSpec {
                    pod_node_name: LookupBuf::from("node_name"),
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
                    log.insert(
                        LookupBuf::from_str("kubernetes.pod_ip").unwrap(),
                        "192.168.1.2",
                    );
                    let ips_vec = vec!["192.168.1.2", "192.168.1.3"];
                    log.insert(LookupBuf::from_str("kubernetes.pod_ips").unwrap(), ips_vec);
                    log
                },
            ),
        ];

        for (fields_spec, pod_status, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_pod_status(&mut log, &fields_spec, &pod_status);
            assert_eq!(log, expected);
        }
    }

    #[test]
    fn test_annotate_from_container_status() {
        let cases = vec![
            (
                FieldsSpec::default(),
                ContainerStatus::default(),
                LogEvent::default(),
            ),
            (
                FieldsSpec {
                    ..FieldsSpec::default()
                },
                ContainerStatus {
                    container_id: Some("container_id_foo".to_owned()),
                    ..ContainerStatus::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(LookupBuf::from_str("kubernetes.container_id").unwrap(), "container_id_foo");
                    log
                },
            ),
        ];
        for (fields_spec, container_status, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container_status(&mut log, &fields_spec, &container_status);
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
                        LookupBuf::from_str("kubernetes.container_image").unwrap(),
                        "sandbox0-container-image",
                    );
                    log
                },
            ),
            (
                FieldsSpec {
                    container_image: LookupBuf::from("container_image"),
                    ..Default::default()
                },
                Container {
                    image: Some("sandbox0-container-image".to_owned()),
                    ..Default::default()
                },
                {
                    let mut log = LogEvent::default();
                    log.insert(
                        LookupBuf::from("container_image"),
                        "sandbox0-container-image",
                    );
                    log
                },
            ),
        ];

        for (fields_spec, container, expected) in cases.into_iter() {
            let mut log = LogEvent::default();
            annotate_from_container(&mut log, &fields_spec, &container);
            assert_eq!(log, expected);
        }
    }
}
