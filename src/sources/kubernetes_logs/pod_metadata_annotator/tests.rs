use std::path::PathBuf;

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
                    metadata_path!("kubernetes_logs", "pod_annotations", "sandbox0-annotation0"),
                    "val0",
                );
                log.insert(
                    metadata_path!("kubernetes_logs", "pod_annotations", "sandbox0-annotation1"),
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
                container_name: OwnedTargetPath::event(owned_value_path!("container_name")).into(),
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

    for (fields_spec, file, expected, log_namespace) in cases.into_iter() {
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
                pod_ip: OwnedTargetPath::event(owned_value_path!("kubernetes", "custom_pod_ip"))
                    .into(),
                pod_ips: OwnedTargetPath::event(owned_value_path!("kubernetes", "custom_pod_ips"))
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
        annotate_from_container_status(&mut log, &fields_spec, &container_status, log_namespace);
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
