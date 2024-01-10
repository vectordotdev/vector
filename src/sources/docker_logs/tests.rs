use crate::sources::docker_logs::*;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<DockerLogsConfig>();
}

#[test]
fn exclude_self() {
    let (tx, _rx) = SourceSender::new_test();
    let mut source = DockerLogsSource::new(
        DockerLogsConfig::default(),
        tx,
        ShutdownSignal::noop(),
        LogNamespace::Legacy,
    )
    .unwrap();
    source.hostname = Some("451062c59603".to_owned());
    assert!(source.exclude_self("451062c59603a1cf0c6af3e74a31c0ae63d8275aa16a5fc78ef31b923baaffc3"));

    // hostname too short
    source.hostname = Some("a".to_owned());
    assert!(!source.exclude_self("a29d569bd46c"));
}

#[cfg(all(test, feature = "docker-logs-integration-tests"))]
mod integration_tests {
    use crate::sources::docker_logs::*;
    use crate::sources::docker_logs::{CONTAINER, CREATED_AT, IMAGE, NAME};
    use crate::{
        event::Event,
        test_util::{
            collect_n, collect_ready,
            components::{assert_source_compliance, SOURCE_TAGS},
            trace_init,
        },
        SourceSender,
    };
    use bollard::{
        container::{
            Config as ContainerConfig, CreateContainerOptions, KillContainerOptions,
            RemoveContainerOptions, StartContainerOptions, WaitContainerOptions,
        },
        image::{CreateImageOptions, ListImagesOptions},
    };
    use futures::{stream::TryStreamExt, FutureExt};
    use similar_asserts::assert_eq;
    use vrl::value;

    /// None if docker is not present on the system
    async fn source_with<'a, L: Into<Option<&'a str>>>(
        names: &[&str],
        label: L,
        log_namespace: Option<bool>,
    ) -> impl Stream<Item = Event> {
        source_with_config(DockerLogsConfig {
            include_containers: Some(names.iter().map(|&s| s.to_owned()).collect()),
            include_labels: Some(label.into().map(|l| vec![l.to_owned()]).unwrap_or_default()),
            log_namespace,
            ..DockerLogsConfig::default()
        })
        .await
    }

    async fn source_with_config(config: DockerLogsConfig) -> impl Stream<Item = Event> + Unpin {
        let (sender, recv) = SourceSender::new_test();
        let source = config
            .build(SourceContext::new_test(sender, None))
            .await
            .unwrap();

        tokio::spawn(async move { source.await.unwrap() });

        recv
    }

    /// Users should ensure to remove container before exiting.
    async fn log_container(
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
        tty: bool,
    ) -> String {
        cmd_container(name, label, vec!["echo", log], docker, tty).await
    }

    /// Users should ensure to remove container before exiting.
    /// Will resend message every so often.
    async fn eternal_container(
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
    ) -> String {
        cmd_container(
            name,
            label,
            vec![
                "sh",
                "-c",
                format!("echo before; i=0; while [ $i -le 50 ]; do sleep 0.1; echo {}; i=$((i+1)); done", log).as_str(),
            ],
            docker,
            false
        ).await
    }

    /// Users should ensure to remove container before exiting.
    async fn cmd_container(
        name: &str,
        label: Option<&str>,
        cmd: Vec<&str>,
        docker: &Docker,
        tty: bool,
    ) -> String {
        if let Some(id) = cmd_container_for_real(name, label, cmd, docker, tty).await {
            id
        } else {
            // Maybe a before created container is present
            info!(
                message = "Assumes that named container remained from previous tests.",
                name = name
            );
            name.to_owned()
        }
    }

    /// Users should ensure to remove container before exiting.
    async fn cmd_container_for_real(
        name: &str,
        label: Option<&str>,
        cmd: Vec<&str>,
        docker: &Docker,
        tty: bool,
    ) -> Option<String> {
        pull_busybox(docker).await;

        trace!("Creating container.");

        let options = Some(CreateContainerOptions {
            name,
            platform: None,
        });
        let config = ContainerConfig {
            image: Some("busybox"),
            cmd: Some(cmd),
            labels: label.map(|label| vec![(label, "")].into_iter().collect()),
            tty: Some(tty),
            ..Default::default()
        };

        let container = docker.create_container(options, config).await;
        container.ok().map(|c| c.id)
    }

    async fn pull_busybox(docker: &Docker) {
        let mut filters = HashMap::new();
        filters.insert("reference", vec!["busybox:latest"]);

        let options = Some(ListImagesOptions {
            filters,
            ..Default::default()
        });

        let images = docker.list_images(options).await.unwrap();
        if images.is_empty() {
            // If `busybox:latest` not found, pull it
            let options = Some(CreateImageOptions {
                from_image: "busybox",
                tag: "latest",
                ..Default::default()
            });

            docker
                .create_image(options, None, None)
                .for_each(|item| async move {
                    let info = item.unwrap();
                    if let Some(error) = info.error {
                        panic!("{:?}", error);
                    }
                })
                .await
        }
    }

    /// Returns once container has started
    async fn container_start(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Starting container.");

        let options = None::<StartContainerOptions<&str>>;
        docker.start_container(id, options).await
    }

    /// Returns once container is done running
    async fn container_wait(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Waiting for container.");

        docker
            .wait_container(id, None::<WaitContainerOptions<&str>>)
            .try_for_each(|exit| async move {
                info!(message = "Container exited with status code.", status_code = ?exit.status_code);
                Ok(())
            })
            .await
    }

    /// Returns once container is killed
    async fn container_kill(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        trace!("Waiting for container to be killed.");

        docker
            .kill_container(id, None::<KillContainerOptions<&str>>)
            .await
    }

    /// Returns once container is done running
    async fn container_run(id: &str, docker: &Docker) -> Result<(), bollard::errors::Error> {
        container_start(id, docker).await?;
        container_wait(id, docker).await
    }

    async fn container_remove(id: &str, docker: &Docker) {
        trace!("Removing container.");

        // Don't panic, as this is unrelated to the test, and there are possibly other containers that need to be removed
        _ = docker
            .remove_container(id, None::<RemoveContainerOptions>)
            .await
            .map_err(|e| error!(%e));
    }

    /// Returns once it's certain that log has been made
    /// Expects that this is the only one with a container
    async fn container_log_n(
        n: usize,
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
    ) -> String {
        container_with_optional_tty_log_n(n, name, label, log, docker, false).await
    }
    async fn container_with_optional_tty_log_n(
        n: usize,
        name: &str,
        label: Option<&str>,
        log: &str,
        docker: &Docker,
        tty: bool,
    ) -> String {
        let id = log_container(name, label, log, docker, tty).await;
        for _ in 0..n {
            if let Err(error) = container_run(&id, docker).await {
                container_remove(&id, docker).await;
                panic!("Container failed to start with error: {:?}", error);
            }
        }
        id
    }

    /// Once function returns, the container has entered into running state.
    /// Container must be killed before removed.
    async fn running_container(
        name: &'static str,
        label: Option<&'static str>,
        log: &'static str,
        docker: &Docker,
    ) -> String {
        let out = source_with(&[name], None, None).await;
        let docker = docker.clone();

        let id = eternal_container(name, label, log, &docker).await;
        if let Err(error) = container_start(&id, &docker).await {
            container_remove(&id, &docker).await;
            panic!("Container start failed with error: {:?}", error);
        }

        // Wait for before message
        let events = collect_n(out, 1).await;
        assert_eq!(
            events[0].as_log()[log_schema().message_key().unwrap().to_string()],
            "before".into()
        );

        id
    }

    fn is_empty<T>(mut rx: impl Stream<Item = T> + Unpin) -> bool {
        rx.next().now_or_never().is_none()
    }

    #[tokio::test]
    async fn container_with_tty_vector_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Vector)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "log container_with_tty";
            let name = "container_with_tty_namespaced";

            let out = source_with(&[name], None, Some(true)).await;

            let docker = docker(None, None).unwrap();

            let id = container_with_optional_tty_log_n(1, name, None, message, &docker, true).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            assert_eq!(events[0].as_log().get(".").unwrap(), &value!(message));
        })
        .await;
    }

    #[tokio::test]
    async fn container_with_tty_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "log container_with_tty";
            let name = "container_with_tty";

            let out = source_with(&[name], None, None).await;

            let docker = docker(None, None).unwrap();

            let id = container_with_optional_tty_log_n(1, name, None, message, &docker, true).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn newly_started_vector_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Vector)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "9";
            let name = "vector_test_newly_started_namespaced";
            let label = "vector_test_label_newly_started";

            let out = source_with(&[name], None, Some(true)).await;

            let docker = docker(None, None).unwrap();

            let id = container_log_n(1, name, Some(label), message, &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);

            let log = events[0].as_log();
            let meta = log.metadata().value();
            assert_eq!(log.get(".").unwrap(), &value!(message));
            assert_eq!(
                meta.get(path!(DockerLogsConfig::NAME, CONTAINER)).unwrap(),
                &value!(id)
            );
            assert!(meta
                .get(path!(DockerLogsConfig::NAME, CREATED_AT))
                .is_some());
            assert_eq!(
                meta.get(path!(DockerLogsConfig::NAME, IMAGE)).unwrap(),
                &value!("busybox")
            );
            assert!(meta
                .get(path!(DockerLogsConfig::NAME, "labels", label))
                .is_some());
            assert_eq!(
                meta.get(path!(DockerLogsConfig::NAME, NAME)).unwrap(),
                &value!(name)
            );
            assert_eq!(
                meta.get(path!("vector", "source_type")).unwrap(),
                &value!(DockerLogsConfig::NAME)
            );
            assert!(meta
                .get(path!("vector", "ingest_timestamp"))
                .unwrap()
                .is_timestamp())
        })
        .await;
    }

    #[tokio::test]
    async fn newly_started_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "9";
            let name = "vector_test_newly_started";
            let label = "vector_test_label_newly_started";

            let out = source_with(&[name], None, None).await;

            let docker = docker(None, None).unwrap();

            let id = container_log_n(1, name, Some(label), message, &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            let log = events[0].as_log();
            assert_eq!(*log.get_message().unwrap(), message.into());
            assert_eq!(log[CONTAINER], id.into());
            assert!(log.get(CREATED_AT).is_some());
            assert_eq!(log[IMAGE], "busybox".into());
            assert!(log.get(format!("label.{}", label).as_str()).is_some());
            assert_eq!(events[0].as_log()[&NAME], name.into());
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                DockerLogsConfig::NAME.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn restart_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "10";
            let name = "vector_test_restart";

            let out = source_with(&[name], None, None).await;

            let docker = docker(None, None).unwrap();

            let id = container_log_n(2, name, None, message, &docker).await;
            let events = collect_n(out, 2).await;
            container_remove(&id, &docker).await;

            let definition = schema_definitions.unwrap();

            definition.assert_valid_for_event(&events[0]);
            let message_key = log_schema().message_key().unwrap().to_string();
            assert_eq!(events[0].as_log()[&message_key], message.into());
            definition.assert_valid_for_event(&events[1]);
            assert_eq!(events[1].as_log()[message_key], message.into());
        })
        .await;
    }

    #[tokio::test]
    async fn include_containers_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "11";
            let name0 = "vector_test_include_container_0";
            let name1 = "vector_test_include_container_1";

            let out = source_with(&[name1], None, None).await;

            let docker = docker(None, None).unwrap();

            let id0 = container_log_n(1, name0, None, "11", &docker).await;
            let id1 = container_log_n(1, name1, None, message, &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id0, &docker).await;
            container_remove(&id1, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn exclude_containers_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let will_be_read = "12";

            let prefix = "vector_test_exclude_containers";
            let included0 = format!("{}_{}", prefix, "include0");
            let included1 = format!("{}_{}", prefix, "include1");
            let excluded0 = format!("{}_{}", prefix, "excluded0");

            let docker = docker(None, None).unwrap();

            let out = source_with_config(DockerLogsConfig {
                include_containers: Some(vec![prefix.to_owned()]),
                exclude_containers: Some(vec![excluded0.to_owned()]),
                ..DockerLogsConfig::default()
            })
            .await;

            let id0 = container_log_n(1, &excluded0, None, "will not be read", &docker).await;
            let id1 = container_log_n(1, &included0, None, will_be_read, &docker).await;
            let id2 = container_log_n(1, &included1, None, will_be_read, &docker).await;
            tokio::time::sleep(Duration::from_secs(1)).await;
            let events = collect_ready(out).await;
            container_remove(&id0, &docker).await;
            container_remove(&id1, &docker).await;
            container_remove(&id2, &docker).await;

            assert_eq!(events.len(), 2);

            let definition = schema_definitions.unwrap();
            definition.assert_valid_for_event(&events[0]);

            let message_key = log_schema().message_key().unwrap().to_string();
            assert_eq!(events[0].as_log()[&message_key], will_be_read.into());

            definition.assert_valid_for_event(&events[1]);
            assert_eq!(events[1].as_log()[message_key], will_be_read.into());
        })
        .await;
    }

    #[tokio::test]
    async fn include_labels_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "13";
            let name0 = "vector_test_include_labels_0";
            let name1 = "vector_test_include_labels_1";
            let label = "vector_test_include_label";

            let out = source_with(&[name0, name1], label, None).await;

            let docker = docker(None, None).unwrap();

            let id0 = container_log_n(1, name0, None, "13", &docker).await;
            let id1 = container_log_n(1, name1, Some(label), message, &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id0, &docker).await;
            container_remove(&id1, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn currently_running_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "14";
            let name = "vector_test_currently_running";
            let label = "vector_test_label_currently_running";

            let docker = docker(None, None).unwrap();
            let id = running_container(name, Some(label), message, &docker).await;
            let out = source_with(&[name], None, None).await;

            let events = collect_n(out, 1).await;
            _ = container_kill(&id, &docker).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            let log = events[0].as_log();
            assert_eq!(*log.get_message().unwrap(), message.into());
            assert_eq!(log[CONTAINER], id.into());
            assert!(log.get(CREATED_AT).is_some());
            assert_eq!(log[IMAGE], "busybox".into());
            assert!(log.get(format!("label.{}", label).as_str()).is_some());
            assert_eq!(events[0].as_log()[&NAME], name.into());
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                DockerLogsConfig::NAME.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn include_image_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "15";
            let name = "vector_test_include_image";
            let config = DockerLogsConfig {
                include_containers: Some(vec![name.to_owned()]),
                include_images: Some(vec!["busybox".to_owned()]),
                ..DockerLogsConfig::default()
            };

            let out = source_with_config(config).await;

            let docker = docker(None, None).unwrap();

            let id = container_log_n(1, name, None, message, &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            assert_eq!(
                events[0].as_log()[log_schema().message_key().unwrap().to_string()],
                message.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn not_include_image_legacy_namespace() {
        trace_init();

        // No assert_source_compliance here since we aren't including the image
        // no events are collected.

        let message = "16";
        let name = "vector_test_not_include_image";
        let config_ex = DockerLogsConfig {
            include_images: Some(vec!["some_image".to_owned()]),
            ..DockerLogsConfig::default()
        };

        let exclude_out = source_with_config(config_ex).await;

        let docker = docker(None, None).unwrap();

        let id = container_log_n(1, name, None, message, &docker).await;
        container_remove(&id, &docker).await;

        assert!(is_empty(exclude_out));
    }

    #[tokio::test]
    async fn not_include_running_image_legacy_namespace() {
        trace_init();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "17";
            let name = "vector_test_not_include_running_image";
            let config_ex = DockerLogsConfig {
                include_images: Some(vec!["some_image".to_owned()]),
                ..DockerLogsConfig::default()
            };
            let config_in = DockerLogsConfig {
                include_containers: Some(vec![name.to_owned()]),
                include_images: Some(vec!["busybox".to_owned()]),
                ..DockerLogsConfig::default()
            };

            let docker = docker(None, None).unwrap();

            let id = running_container(name, None, message, &docker).await;
            let exclude_out = source_with_config(config_ex).await;
            let include_out = source_with_config(config_in).await;

            _ = collect_n(include_out, 1).await;
            _ = container_kill(&id, &docker).await;
            container_remove(&id, &docker).await;

            assert!(is_empty(exclude_out));
        })
        .await;
    }

    #[tokio::test]
    async fn flat_labels_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let message = "18";
            let name = "vector_test_flat_labels";
            let label = "vector.test.label.flat.labels";

            let docker = docker(None, None).unwrap();
            let id = running_container(name, Some(label), message, &docker).await;
            let out = source_with(&[name], None, None).await;

            let events = collect_n(out, 1).await;
            _ = container_kill(&id, &docker).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            let log = events[0].as_log();
            assert_eq!(*log.get_message().unwrap(), message.into());
            assert_eq!(log[CONTAINER], id.into());
            assert!(log.get(CREATED_AT).is_some());
            assert_eq!(log[IMAGE], "busybox".into());
            assert!(log
                .get("label")
                .unwrap()
                .as_object()
                .unwrap()
                .get(label)
                .is_some());
            assert_eq!(events[0].as_log()[&NAME], name.into());
            assert_eq!(
                events[0].as_log()[log_schema().source_type_key().unwrap().to_string()],
                DockerLogsConfig::NAME.into()
            );
        })
        .await;
    }

    #[tokio::test]
    async fn log_longer_than_16kb_legacy_namespace() {
        trace_init();
        let schema_definitions = DockerLogsConfig::default()
            .outputs(LogNamespace::Legacy)
            .first()
            .unwrap()
            .schema_definition
            .clone();

        assert_source_compliance(&SOURCE_TAGS, async {
            let mut message = String::with_capacity(20 * 1024);
            for _ in 0..message.capacity() {
                message.push('0');
            }
            let name = "vector_test_log_longer_than_16kb";

            let out = source_with(&[name], None, None).await;

            let docker = docker(None, None).unwrap();

            let id = container_log_n(1, name, None, message.as_str(), &docker).await;
            let events = collect_n(out, 1).await;
            container_remove(&id, &docker).await;

            schema_definitions
                .unwrap()
                .assert_valid_for_event(&events[0]);
            let log = events[0].as_log();
            assert_eq!(*log.get_message().unwrap(), message.into());
        })
        .await;
    }

    #[tokio::test]
    async fn merge_multiline_vector_namespace() {
        assert_source_compliance(&SOURCE_TAGS, async {
            trace_init();
            let schema_definitions = DockerLogsConfig::default()
                .outputs(LogNamespace::Vector)
                .first()
                .unwrap()
                .schema_definition
                .clone()
                .unwrap();

            let emitted_messages = vec![
                "java.lang.Exception",
                "    at com.foo.bar(bar.java:123)",
                "    at com.foo.baz(baz.java:456)",
            ];
            let expected_messages = vec![concat!(
                "java.lang.Exception\n",
                "    at com.foo.bar(bar.java:123)\n",
                "    at com.foo.baz(baz.java:456)",
            )];
            let name = "vector_test_merge_multiline_namespaced";
            let config = DockerLogsConfig {
                include_containers: Some(vec![name.to_owned()]),
                include_images: Some(vec!["busybox".to_owned()]),
                multiline: Some(MultilineConfig {
                    start_pattern: "^[^\\s]".to_owned(),
                    condition_pattern: "^[\\s]+at".to_owned(),
                    mode: line_agg::Mode::ContinueThrough,
                    timeout_ms: Duration::from_millis(10),
                }),
                log_namespace: Some(true),
                ..DockerLogsConfig::default()
            };

            let out = source_with_config(config).await;

            let docker = docker(None, None).unwrap();

            let command = emitted_messages
                .into_iter()
                .map(|message| format!("echo {:?}", message))
                .collect::<Box<_>>()
                .join(" && ");

            let id = cmd_container(name, None, vec!["sh", "-c", &command], &docker, false).await;
            if let Err(error) = container_run(&id, &docker).await {
                container_remove(&id, &docker).await;
                panic!("Container failed to start with error: {:?}", error);
            }
            let events = collect_n(out, expected_messages.len()).await;
            container_remove(&id, &docker).await;

            let actual_messages = events
                .into_iter()
                .map(|event| {
                    schema_definitions.assert_valid_for_event(&event);

                    event
                        .into_log()
                        .remove(".")
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_messages, expected_messages);
        })
        .await;
    }

    #[tokio::test]
    async fn merge_multiline_legacy_namespace() {
        assert_source_compliance(&SOURCE_TAGS, async {
            trace_init();
            let schema_definitions = DockerLogsConfig::default()
                .outputs(LogNamespace::Legacy)
                .first()
                .unwrap()
                .schema_definition
                .clone()
                .unwrap();

            let emitted_messages = vec![
                "java.lang.Exception",
                "    at com.foo.bar(bar.java:123)",
                "    at com.foo.baz(baz.java:456)",
            ];
            let expected_messages = vec![concat!(
                "java.lang.Exception\n",
                "    at com.foo.bar(bar.java:123)\n",
                "    at com.foo.baz(baz.java:456)",
            )];
            let name = "vector_test_merge_multiline";
            let config = DockerLogsConfig {
                include_containers: Some(vec![name.to_owned()]),
                include_images: Some(vec!["busybox".to_owned()]),
                multiline: Some(MultilineConfig {
                    start_pattern: "^[^\\s]".to_owned(),
                    condition_pattern: "^[\\s]+at".to_owned(),
                    mode: line_agg::Mode::ContinueThrough,
                    timeout_ms: Duration::from_millis(10),
                }),
                ..DockerLogsConfig::default()
            };

            let out = source_with_config(config).await;

            let docker = docker(None, None).unwrap();

            let command = emitted_messages
                .into_iter()
                .map(|message| format!("echo {:?}", message))
                .collect::<Box<_>>()
                .join(" && ");

            let id = cmd_container(name, None, vec!["sh", "-c", &command], &docker, false).await;
            if let Err(error) = container_run(&id, &docker).await {
                container_remove(&id, &docker).await;
                panic!("Container failed to start with error: {:?}", error);
            }
            let events = collect_n(out, expected_messages.len()).await;
            container_remove(&id, &docker).await;

            let actual_messages = events
                .into_iter()
                .map(|event| {
                    schema_definitions.assert_valid_for_event(&event);

                    event
                        .into_log()
                        .remove(log_schema().message_key_target_path().unwrap())
                        .unwrap()
                        .to_string_lossy()
                        .into_owned()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual_messages, expected_messages);
        })
        .await;
    }
}
