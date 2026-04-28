mod tests {
    use bytes::Bytes;
    use chrono::Utc;
    use futures::{FutureExt, StreamExt, pin_mut};
    use http_1::{Request, Response};
    use k8s_openapi::api::core::v1::{Namespace, Node, Pod};
    use kube::{
        Client,
        api::{ListMeta, ObjectList, TypeMeta, WatchEvent},
        client::Body,
    };
    use similar_asserts::assert_eq;
    use std::{
        fs::{self, File},
        future::Future,
        io::Write,
        path::{Path, PathBuf},
    };
    use tempfile::tempdir;
    use tokio::time::{Duration, sleep, timeout};
    use tower_test::mock::{Handle, SendResponse};
    use vector_lib::{
        config::{
            AcknowledgementsConfig, GlobalOptions, LogNamespace, SourceAcknowledgementsConfig,
        },
        id::ComponentKey,
        lookup::{OwnedTargetPath, owned_value_path},
        schema::Definition,
    };
    use vrl::value::{Kind, kind::Collection};

    use crate::{
        SourceSender,
        config::SourceConfig,
        event::{Event, EventStatus},
        shutdown::ShutdownSignal,
        test_util::components::{SOURCE_TAGS, assert_source_compliance},
    };

    use super::super::{Config, Source};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<Config>();
    }

    #[test]
    fn prepare_exclude_paths() {
        let cases = vec![
            (
                Config::default(),
                vec![
                    glob::Pattern::new("**/*.gz").unwrap(),
                    glob::Pattern::new("**/*.tmp").unwrap(),
                ],
            ),
            (
                Config {
                    exclude_paths_glob_patterns: vec![std::path::PathBuf::from("**/*.tmp")],
                    ..Default::default()
                },
                vec![glob::Pattern::new("**/*.tmp").unwrap()],
            ),
            (
                Config {
                    exclude_paths_glob_patterns: vec![
                        std::path::PathBuf::from("**/kube-system_*/**"),
                        std::path::PathBuf::from("**/*.gz"),
                        std::path::PathBuf::from("**/*.tmp"),
                    ],
                    ..Default::default()
                },
                vec![
                    glob::Pattern::new("**/kube-system_*/**").unwrap(),
                    glob::Pattern::new("**/*.gz").unwrap(),
                    glob::Pattern::new("**/*.tmp").unwrap(),
                ],
            ),
        ];

        for (input, mut expected) in cases {
            let mut output = super::super::prepare_exclude_paths(&input).unwrap();
            expected.sort();
            output.sort();
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn prepare_field_selector() {
        let cases = vec![
            // We're not testing `Config::default()` or empty `self_node_name`
            // as passing env vars in the concurrent tests is difficult.
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe",
            ),
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    extra_field_selector: "".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe",
            ),
            (
                Config {
                    self_node_name: "qwe".to_owned(),
                    extra_field_selector: "foo=bar".to_owned(),
                    ..Default::default()
                },
                "spec.nodeName=qwe,foo=bar",
            ),
        ];

        for (input, expected) in cases {
            let output = super::super::prepare_field_selector(&input, "qwe").unwrap();
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn prepare_label_selector() {
        let cases = vec![
            (
                Config::default().extra_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config::default().extra_namespace_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_label_selector: "".to_owned(),
                    ..Default::default()
                }
                .extra_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_namespace_label_selector: "".to_owned(),
                    ..Default::default()
                }
                .extra_namespace_label_selector,
                "vector.dev/exclude!=true",
            ),
            (
                Config {
                    extra_label_selector: "qwe".to_owned(),
                    ..Default::default()
                }
                .extra_label_selector,
                "vector.dev/exclude!=true,qwe",
            ),
            (
                Config {
                    extra_namespace_label_selector: "qwe".to_owned(),
                    ..Default::default()
                }
                .extra_namespace_label_selector,
                "vector.dev/exclude!=true,qwe",
            ),
        ];

        for (input, expected) in cases {
            let output = super::super::prepare_label_selector(&input);
            assert_eq!(expected, output, "expected left, actual right");
        }
    }

    #[test]
    fn test_output_schema_definition_vector_namespace() {
        let definitions = toml::from_str::<Config>("")
            .unwrap()
            .outputs(LogNamespace::Vector)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "file"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_id"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_image"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "container_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "namespace_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "node_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_annotations"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_ip"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_ips"),
                        Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_labels"),
                        Kind::object(Collection::empty().with_unknown(Kind::bytes()))
                            .or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_namespace"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_node_name"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_owner"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "pod_uid"),
                        Kind::bytes().or_undefined(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "stream"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("kubernetes_logs", "timestamp"),
                        Kind::timestamp(),
                        Some("timestamp")
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "source_type"),
                        Kind::bytes(),
                        None
                    )
                    .with_metadata_field(
                        &owned_value_path!("vector", "ingest_timestamp"),
                        Kind::timestamp(),
                        None
                    )
                    .with_meaning(OwnedTargetPath::event_root(), "message")
            )
        )
    }

    #[test]
    fn test_output_schema_definition_legacy_namespace() {
        let definitions = toml::from_str::<Config>("")
            .unwrap()
            .outputs(LogNamespace::Legacy)
            .remove(0)
            .schema_definition(true);

        assert_eq!(
            definitions,
            Some(
                Definition::new_with_default_metadata(
                    Kind::object(Collection::empty()),
                    [LogNamespace::Legacy]
                )
                .with_event_field(&owned_value_path!("file"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("message"),
                    Kind::bytes(),
                    Some("message")
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_id"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_image"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "container_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "namespace_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "node_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_annotations"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_ip"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_ips"),
                    Kind::array(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_labels"),
                    Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_namespace"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_node_name"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_owner"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(
                    &owned_value_path!("kubernetes", "pod_uid"),
                    Kind::bytes().or_undefined(),
                    None
                )
                .with_event_field(&owned_value_path!("stream"), Kind::bytes(), None)
                .with_event_field(
                    &owned_value_path!("timestamp"),
                    Kind::timestamp(),
                    Some("timestamp")
                )
                .with_event_field(
                    &owned_value_path!("source_type"),
                    Kind::bytes(),
                    None
                )
            )
        )
    }

    #[tokio::test]
    async fn file_start_position_server_restart_with_file_rotation_no_acknowledge() {
        file_start_position_server_restart_with_file_rotation(NoAcks).await
    }

    #[tokio::test]
    async fn file_start_position_server_restart_with_file_rotation_acknowledged() {
        file_start_position_server_restart_with_file_rotation(Acks).await
    }

    /// Verifies that checkpoints do NOT advance until acks are received.
    /// When events are rejected (not acknowledged), the source should
    /// re-read the same data on restart.
    #[tokio::test]
    async fn checkpoint_does_not_advance_without_ack() {
        let (mock_service, handle) = tower_test::mock::pair::<Request<Body>, Response<Body>>();
        let ns_name = "default";
        let container_name = "test";
        let pod_uid = "dd3448e2-60bb-46ab-bd34-d42b61be366d";
        let pod_name = "test";
        let node_name = "test";
        tokio::spawn(get_mock_future(
            handle,
            ns_name,
            pod_name,
            pod_uid,
            container_name,
        ));

        let tmp_dir = tempdir().unwrap();
        let dir = &format!(
            "{}/{}_{}_{}/{}",
            tmp_dir.path().to_str().unwrap(),
            ns_name,
            pod_name,
            pod_uid,
            container_name
        );
        let dir_path = Path::new(dir);
        fs::create_dir_all(dir_path).unwrap();
        let mut config = Config {
            self_node_name: node_name.to_owned(),
            glob_minimum_cooldown_ms: Duration::from_millis(100),
            ..Default::default()
        };

        let path = dir_path.join("log.log");
        let first_file = File::create(&path).unwrap();
        sleep_500_millis().await;
        writeln!(
            &first_file,
            "2016-10-06T00:17:09.669794202Z stdout F first line"
        )
        .unwrap();

        // Run server with acks enabled but events rejected (not acknowledged).
        // Checkpoints should NOT advance.
        {
            let received = run_kubernetes_source_with_status(
                &mut config,
                false, // don't wait for shutdown (it may hang without acks)
                EventStatus::Rejected,
                async {
                    sleep_500_millis().await;
                },
                Client::new(mock_service.clone(), ns_name),
                dir_path.to_path_buf(),
                tmp_dir.path().to_str().unwrap(),
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["first line"]);
        }

        // Restart the server. Since the checkpoint did NOT advance
        // (events were rejected), we should re-read "first line".
        {
            let received = run_kubernetes_source(
                &mut config,
                true,
                Acks,
                async {
                    sleep_500_millis().await;
                },
                Client::new(mock_service.clone(), ns_name),
                dir_path.to_path_buf(),
                tmp_dir.path().to_str().unwrap(),
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(
                lines,
                vec!["first line"],
                "checkpoint should not have advanced when events were rejected"
            );
        }

        fs::remove_dir_all(dir_path).unwrap();
    }

    async fn get_mock_future(
        handle: Handle<Request<Body>, Response<Body>>,
        namespace_name: &str,
        pod_name: &str,
        pod_uid: &str,
        container_name: &str,
    ) {
        // Receive a request for pods/namespaces/nodes and respond with some data
        pin_mut!(handle);
        let mut pod_count = 0;
        let mut ns_count = 0;
        let mut node_count = 0;
        loop {
            let (request, send) = handle.next_request().await.expect("service not called");
            assert_eq!(request.method(), http_1::Method::GET);
            let request_uri = request.uri().to_string();
            if !request_uri.contains("watch=true") {
                // we're back to the initial listing, possibly due to file server restarting
                pod_count = 0;
                ns_count = 0;
                node_count = 0;
            }
            if request_uri.starts_with("/api/v1/pods") {
                pod_count = handle_pod(
                    request_uri,
                    send,
                    namespace_name,
                    pod_name,
                    pod_uid,
                    container_name,
                    pod_count,
                );
            } else if request_uri.starts_with("/api/v1/namespaces") {
                ns_count = handle_ns(request_uri, send, namespace_name, ns_count);
            } else if request_uri.starts_with("/api/v1/nodes") {
                node_count = handle_node(request_uri, send, node_count);
            } else {
                panic!("Got unexpected uri in request: {:?}", request_uri);
            }
        }
    }

    fn handle_pod(
        request_uri: String,
        send: SendResponse<Response<Body>>,
        namespace_name: &str,
        pod_name: &str,
        pod_uid: &str,
        container_name: &str,
        pod_count: i32,
    ) -> i32 {
        let timestamp = Utc::now();
        // bump resource_version once we're done with Init so we actually pick up the Apply
        let mode = if request_uri
            == "/api/v1/pods?&fieldSelector=spec.nodeName%3Dtest&labelSelector=vector.dev%2Fexclude%21%3Dtrue&limit=500"
        {
            "list"
        } else {
            "watch"
        };
        let resource_version = format!("{}", if mode == "list" { 0 } else { 1 });
        let pod: Pod = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Pod",
            "metadata": {
                "name": pod_name,
                "annotations": { "kube-rs": "test" },
                "resourceVersion": resource_version,
                "namespace": Some(namespace_name),
                "uid": Some(pod_uid),
            },
            "spec": {
                "containers": [{ "name": container_name, "image": "test-image" }],
            },
            "status": {
                "phase": "Running",
                "conditions": [
                    {"type": "Ready", "status": "True", "lastProbeTime": timestamp, "lastTransitionTime": timestamp},
                    {"type": "PodReadyToStartContainers", "status": "True", "lastProbeTime": timestamp, "lastTransitionTime": timestamp},
                    {"type": "Initialized", "status": "True", "lastProbeTime": timestamp, "lastTransitionTime": timestamp},
                    {"type": "ContainersReady", "status": "True", "lastProbeTime": timestamp, "lastTransitionTime": timestamp},
                    {"type": "PodScheduled", "status": "True", "lastProbeTime": timestamp, "lastTransitionTime": timestamp},
                ],
                "containerStatuses": [
                    { "image": "test-image", "image_id": "foo", "name": "test", "ready": true, "state": {"running": {}}}
                ]
            }
        }))
            .unwrap();
        if mode == "list" {
            send.send_response(
                Response::builder()
                    .body(Body::from(
                        serde_json::to_vec(&ObjectList {
                            types: TypeMeta {
                                api_version: "v1".to_owned(),
                                kind: "Pod".to_owned(),
                            },
                            items: vec![pod],
                            metadata: ListMeta {
                                continue_: None,
                                remaining_item_count: None,
                                resource_version: Some(resource_version),
                                self_link: None,
                            },
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            );
            pod_count + 1
        } else if pod_count == 1 {
            send.send_response(
                Response::builder()
                    .body(Body::from(Bytes::from(
                        serde_json::to_string(&WatchEvent::Modified(pod)).unwrap(),
                    )))
                    .unwrap(),
            );
            pod_count + 1
        } else {
            // don't keep generating more events once we've done the minimal initial list plus one apply
            pod_count
        }
    }

    fn handle_ns(
        request_uri: String,
        send: SendResponse<Response<Body>>,
        namespace_name: &str,
        ns_count: i32,
    ) -> i32 {
        // bump resource_version once we're done with Init so we actually pick up the Apply
        let mode = if request_uri
            == "/api/v1/namespaces?&labelSelector=vector.dev%2Fexclude%21%3Dtrue&limit=500"
        {
            "list"
        } else {
            "watch"
        };
        let resource_version = format!("{}", if mode == "list" { 0 } else { 1 });
        let ns: Namespace = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Namespace",
            "metadata": {
                "name": namespace_name,
                "annotations": { "kube-rs": "test" },
                "resourceVersion": resource_version,
            },
            "status": {
                "phase": "Active"
            }
        }))
        .unwrap();
        if mode == "list" {
            send.send_response(
                Response::builder()
                    .body(Body::from(
                        serde_json::to_vec(&ObjectList {
                            types: TypeMeta {
                                api_version: "v1".to_owned(),
                                kind: "Namespace".to_owned(),
                            },
                            items: vec![ns],
                            metadata: ListMeta {
                                continue_: None,
                                remaining_item_count: None,
                                resource_version: Some(resource_version),
                                self_link: None,
                            },
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            );
            ns_count + 1
        } else if ns_count == 1 {
            send.send_response(
                Response::builder()
                    .body(Body::from(Bytes::from(
                        serde_json::to_string(&WatchEvent::Modified(ns)).unwrap(),
                    )))
                    .unwrap(),
            );
            ns_count + 1
        } else {
            // don't keep generating more events once we've done the minimal initial list plus one apply
            ns_count
        }
    }

    fn handle_node(
        request_uri: String,
        send: SendResponse<Response<Body>>,
        node_count: i32,
    ) -> i32 {
        // bump resource_version once we're done with Init so we actually pick up the Apply
        let mode = if request_uri == "/api/v1/nodes?&fieldSelector=metadata.name%3Dtest&limit=500" {
            "list"
        } else {
            "watch"
        };
        let resource_version = format!("{}", if mode == "list" { 0 } else { 1 });
        let node: Node = serde_json::from_value(serde_json::json!({
            "apiVersion": "v1",
            "kind": "Node",
            "metadata": {
                "name": "1.2.3.4",
                "annotations": { "kube-rs": "test" },
                "labels": {
                    "name": "foo"
                },
                "resourceVersion": resource_version,
            },
        }))
        .unwrap();
        if mode == "list" {
            send.send_response(
                Response::builder()
                    .body(Body::from(
                        serde_json::to_vec(&ObjectList {
                            types: TypeMeta {
                                api_version: "v1".to_owned(),
                                kind: "Node".to_owned(),
                            },
                            items: vec![node],
                            metadata: ListMeta {
                                continue_: None,
                                remaining_item_count: None,
                                resource_version: Some(resource_version),
                                self_link: None,
                            },
                        })
                        .unwrap(),
                    ))
                    .unwrap(),
            );
            node_count + 1
        } else if node_count == 1 {
            send.send_response(
                Response::builder()
                    .body(Body::from(Bytes::from(
                        serde_json::to_string(&WatchEvent::Modified(node)).unwrap(),
                    )))
                    .unwrap(),
            );
            node_count + 1
        } else {
            // don't keep generating more events once we've done the minimal initial list plus one apply
            node_count
        }
    }

    async fn file_start_position_server_restart_with_file_rotation(acking: AckingMode) {
        let (mock_service, handle) = tower_test::mock::pair::<Request<Body>, Response<Body>>();
        let ns_name = "default";
        let container_name = "test";
        let pod_uid = "dd3448e2-60bb-46ab-bd34-d42b61be366d";
        let pod_name = "test";
        let node_name = "test";
        tokio::spawn(get_mock_future(
            handle,
            ns_name,
            pod_name,
            pod_uid,
            container_name,
        ));

        let tmp_dir = tempdir().unwrap();
        let dir = &format!(
            "{}/{}_{}_{}/{}",
            tmp_dir.path().to_str().unwrap(),
            ns_name,
            pod_name,
            pod_uid,
            container_name
        );
        let dir_path = Path::new(dir);
        fs::create_dir_all(dir_path).unwrap();
        let mut config = Config {
            self_node_name: node_name.to_owned(),
            // needs to be < the 500 millis we sleep in the inner async block in the calls to run_kubernetes_source
            glob_minimum_cooldown_ms: Duration::from_millis(100),
            ..Default::default()
        };

        let path = dir_path.join("log.log");
        let path_for_old_file = dir_path.join("log.old");
        let first_file = File::create(&path).unwrap();
        sleep_500_millis().await;
        writeln!(
            &first_file,
            "2016-10-06T00:17:09.669794202Z stdout F first line"
        )
        .unwrap();
        // Run server first time, collect some lines.
        {
            let received = run_kubernetes_source(
                &mut config,
                true,
                acking,
                async {
                    sleep_500_millis().await;
                },
                Client::new(mock_service.clone(), ns_name),
                dir_path.to_path_buf(),
                tmp_dir.path().to_str().unwrap(),
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["first line"]);
        }
        // Perform 'file rotation' to archive old lines.
        fs::rename(path.clone(), &path_for_old_file).expect("could not rename");

        // Restart the server and make sure it does not re-read the old file
        // even though it has a new name.
        let second_file = File::create(&path).unwrap();
        sleep_500_millis().await;
        writeln!(
            &second_file,
            "2016-10-06T00:17:10.669794202Z stdout F second line"
        )
        .unwrap();
        {
            let received = run_kubernetes_source(
                &mut config,
                true,
                acking,
                async {
                    sleep_500_millis().await;
                },
                Client::new(mock_service.clone(), ns_name),
                dir_path.to_path_buf(),
                tmp_dir.path().to_str().unwrap(),
            )
            .await;

            let lines = extract_messages_string(received);
            assert_eq!(lines, vec!["second line"]);
        }

        fs::remove_dir_all(dir_path).unwrap();
    }

    async fn sleep_500_millis() {
        sleep(Duration::from_millis(500)).await;
    }

    fn extract_messages_string(received: Vec<Event>) -> Vec<String> {
        received
            .into_iter()
            .map(Event::into_log)
            .map(|log| log.get_message().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum AckingMode {
        NoAcks, // No acknowledgement handling and no finalization
        Acks,   // Full acknowledgements and proper finalization
    }
    use AckingMode::*;

    async fn run_kubernetes_source(
        config: &mut Config,
        wait_shutdown: bool,
        acking_mode: AckingMode,
        inner: impl Future<Output = ()>,
        client: Client,
        data_dir: PathBuf,
        logs_dir: &str,
    ) -> Vec<Event> {
        let acks = acking_mode == Acks;
        assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = if acks {
                let (tx, rx) = SourceSender::new_test_finalize(EventStatus::Delivered);
                (tx, rx.boxed())
            } else {
                let (tx, rx) = SourceSender::new_test();
                (tx, rx.boxed())
            };

            let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();

            config.acknowledgements = SourceAcknowledgementsConfig::from(acks);
            let globals = GlobalOptions {
                data_dir: Some(data_dir.clone()),
                log_schema: Default::default(),
                telemetry: Default::default(),
                timezone: Default::default(),
                proxy: Default::default(),
                acknowledgements: AcknowledgementsConfig::from(acks),
                expire_metrics: Default::default(),
                expire_metrics_secs: Default::default(),
                expire_metrics_per_metric_set: Default::default(),
                wildcard_matching: Default::default(),
                buffer_utilization_ewma_half_life_seconds: Default::default(),
                latency_ewma_alpha: Default::default(),
                metrics_storage_refresh_period: Default::default(),
            };
            let key = ComponentKey::from("default");
            let log_namespace = LogNamespace::Legacy;

            let source_inner =
                Source::new_test(config, &globals, &key, acks, client, logs_dir.to_owned())
                    .await
                    .unwrap();

            let source = Box::pin(source_inner.run(tx, shutdown, log_namespace).map(|result| {
                result.map_err(|error| {
                    error!(message = "Source future failed.", %error);
                })
            }));

            tokio::spawn(source);

            inner.await;

            drop(trigger_shutdown);

            let result = timeout(Duration::from_secs(5), rx.collect::<Vec<_>>())
                .await
                .expect(
                    "Unclosed channel: may indicate file-server could not shutdown gracefully.",
                );
            if wait_shutdown {
                shutdown_done.await;
            }

            result
        })
        .await
    }

    /// Like [`run_kubernetes_source`] but with a custom event status for ack
    /// finalization. This allows testing the behavior when events are rejected.
    async fn run_kubernetes_source_with_status(
        config: &mut Config,
        wait_shutdown: bool,
        status: EventStatus,
        inner: impl Future<Output = ()>,
        client: Client,
        data_dir: PathBuf,
        logs_dir: &str,
    ) -> Vec<Event> {
        assert_source_compliance(&SOURCE_TAGS, async move {
            let (tx, rx) = SourceSender::new_test_finalize(status);
            let rx = rx.boxed();

            let (trigger_shutdown, shutdown, shutdown_done) = ShutdownSignal::new_wired();

            config.acknowledgements = SourceAcknowledgementsConfig::from(true);
            let globals = GlobalOptions {
                data_dir: Some(data_dir.clone()),
                log_schema: Default::default(),
                telemetry: Default::default(),
                timezone: Default::default(),
                proxy: Default::default(),
                acknowledgements: AcknowledgementsConfig::from(true),
                expire_metrics: Default::default(),
                expire_metrics_secs: Default::default(),
                expire_metrics_per_metric_set: Default::default(),
                wildcard_matching: Default::default(),
                buffer_utilization_ewma_half_life_seconds: Default::default(),
                latency_ewma_alpha: Default::default(),
                metrics_storage_refresh_period: Default::default(),
            };
            let key = ComponentKey::from("default");
            let log_namespace = LogNamespace::Legacy;

            let source_inner = Source::new_test(
                config,
                &globals,
                &key,
                true, // acks enabled
                client,
                logs_dir.to_owned(),
            )
            .await
            .unwrap();

            let source = Box::pin(source_inner.run(tx, shutdown, log_namespace).map(|result| {
                result.map_err(|error| {
                    error!(message = "Source future failed.", %error);
                })
            }));

            tokio::spawn(source);

            inner.await;

            drop(trigger_shutdown);

            // When events are rejected, the finalizer won't complete
            // the ack stream, so use a timeout-based collection.
            let result = rx
                .take_until(tokio::time::sleep(Duration::from_secs(5)))
                .collect::<Vec<_>>()
                .await;
            if wait_shutdown {
                shutdown_done.await;
            }

            result
        })
        .await
    }
}
