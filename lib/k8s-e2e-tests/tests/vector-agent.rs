use std::{
    collections::{BTreeMap, HashSet},
    iter::FromIterator,
    str::FromStr,
};

use futures::{SinkExt, StreamExt};
use indoc::indoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{
    lock, namespace, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};
use tracing::{debug, info};

const HELM_VALUES_AGENT: &str = indoc! {r#"
    role: "Agent"
"#};

const HELM_VALUES_EXISTING_CONFIGMAP: &str = indoc! {r#"
    role: "Agent"
    existingConfigMaps:
    - vector-agent-config
    dataDir: /vector-data-dir
    service:
      ports:
      - name: prom-exporter
        port: 9090
        protocol: TCP
"#};

const CUSTOM_RESOURCE_VECTOR_CONFIG: &str = indoc! {r#"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: vector-agent-config
    data:
      vector.yaml: |
        data_dir: "/vector-data-dir"

        api:
          enabled: false

        sources:
          kubernetes_logs:
            type: "kubernetes_logs"
            ingestion_timestamp_field: ".ingest_timestamp"

        sinks:
          stdout:
            type: "console"
            inputs:
              - "kubernetes_logs"
            encoding:
              codec: "json"
"#};

/// This test validates that vector picks up logs at the simplest case
/// possible - a new pod is deployed and prints to stdout, and we assert that
/// vector picks that up
#[tokio::test]
async fn default_agent() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly merges a log message that
/// kubernetes has internally split into multiple partial log lines.
#[tokio::test]
async fn partial_merge() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_message = generate_long_string(8, 8 * 1024); // 64 KiB
    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            &format!("echo {}", test_message),
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_expected_line = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure the message we got matches the one we emitted.
        assert_eq!(val["message"], test_message);

        if got_expected_line {
            // We've already seen our expected line once! This is not good, we
            // only emitted one.
            panic!("Test message seen more than once");
        }

        // If we did, remember it.
        got_expected_line = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;
    assert!(got_expected_line);

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector picks up preexisting logs - logs that
/// existed before vector was deployed.
#[tokio::test]
async fn preexisting() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let framework = make_framework();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let override_name = get_override_name(&namespace, "vector-agent");
    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Wait for some extra time to ensure pod completes.
    tokio::time::sleep(std::time::Duration::from_secs(10)).await;

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector picks up multiple log lines, and that
/// they arrive at the proper order.
#[tokio::test]
async fn multiple_lines() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let override_name = get_override_name(&namespace, "vector-agent");

    let framework = make_framework();

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_messages = vec!["MARKER1", "MARKER2", "MARKER3", "MARKER4", "MARKER5"];
    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            &format!("echo -e {}", test_messages.join(r"\\n")),
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut test_messages_iter = test_messages.into_iter().peekable();
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Take the next marker.
        let current_marker = test_messages_iter
            .next()
            .expect("expected no more lines since the test messages iter is exhausted");

        // Ensure we got the marker.
        assert_eq!(val["message"], current_marker);

        if test_messages_iter.peek().is_some() {
            // We're not done yet, so go on.
            return FlowControlCommand::GoOn;
        }

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    assert!(test_messages_iter.next().is_none());

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly annotates log events with pod
/// and namespace metadata obtained from the k8s API.
#[tokio::test]
async fn metadata_annotation() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let override_name = get_override_name(&namespace, "vector-agent");
    let framework = make_framework();

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    // Set label on all nodes to check it later.
    framework.label_nodes("label5=foobazbar").await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(
                pod_namespace.clone(),
                Some(BTreeMap::from_iter(
                    [
                        ("label3".to_string(), "foobar".to_string()),
                        ("label4".to_string(), "fizzbuzz".to_string()),
                    ]
                    .into_iter(),
                )),
            ),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![("label1", "hello"), ("label2", "world")],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;
    let k8s_version = framework.kubernetes_version().await?;

    // Replace all non numeric chars from the version number
    let numeric_regex = regex::Regex::new(r#"[^\d]"#).unwrap();
    let minor = k8s_version.minor();
    let numeric_minor = numeric_regex.replace(&minor, "");
    let minor = u8::from_str(&numeric_minor).expect(&format!(
        "Couldn't get u8 from String, received {} instead!",
        k8s_version.minor()
    ));

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Assert pod the event is properly annotated with pod metadata.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod");
        // We've already asserted this above, but repeat for completeness.
        assert_eq!(val["kubernetes"]["pod_namespace"], pod_namespace.as_str());
        assert_eq!(val["kubernetes"]["pod_uid"].as_str().unwrap().len(), 36); // 36 is a standard UUID string length
        assert_eq!(val["kubernetes"]["pod_labels"]["label1"], "hello");
        assert_eq!(val["kubernetes"]["pod_labels"]["label2"], "world");
        assert_eq!(val["kubernetes"]["namespace_labels"]["label3"], "foobar");
        assert_eq!(val["kubernetes"]["namespace_labels"]["label4"], "fizzbuzz");
        assert_eq!(val["kubernetes"]["node_labels"]["label5"], "foobazbar");

        if minor < 16 {
            assert!(val["kubernetes"]["pod_ip"].is_string());
        } else {
            assert!(val["kubernetes"]["pod_ip"].is_string());
            assert!(!val["kubernetes"]["pod_ips"]
                .as_array()
                .expect("Couldn't take array from expected vec")
                .is_empty());
        }
        // We don't have the node name to compare this to, so just assert it's
        // a non-empty string.
        assert!(!val["kubernetes"]["pod_node_name"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(val["kubernetes"]["container_name"], "test-pod");
        assert!(!val["kubernetes"]["container_id"]
            .as_str()
            .unwrap()
            .is_empty());
        assert_eq!(val["kubernetes"]["container_image"], BUSYBOX_IMAGE);

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly filters out the logs that are
/// requested to be excluded from collection, based on k8s API `Pod` labels.
#[tokio::test]
async fn pod_filtering() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let affinity_label = format!("{}-affinity", pod_namespace);
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let affinity_ns_name = format!("{}-affinity", pod_namespace);
    let affinity_ns = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(affinity_ns_name.clone(), None),
        )?)
        .await?;
    let affinity_pod = create_affinity_pod(&framework, &affinity_ns_name, &affinity_label).await?;

    let excluded_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod_with_affinity(
            &pod_namespace,
            "test-pod-excluded",
            "echo EXCLUDED_MARKER",
            vec![("vector.dev/exclude", "true")],
            vec![],
            Some((&affinity_label, "yes")),
            Some(&affinity_ns_name),
        ))?)
        .await?;

    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod-excluded"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Create this pod with affinity to the previous one to ensure they are deployed on the same
    // node.
    let control_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod_with_affinity(
            &pod_namespace,
            "test-pod-control",
            "echo CONTROL_MARKER",
            vec![],
            vec![],
            Some((&affinity_label, "yes")),
            Some(&affinity_ns_name),
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod-control"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(
            &pod_namespace,
            "test-pod-control",
            &namespace,
            &override_name,
        )
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill().await?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        debug!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up == 0 {
            info!("Giving up");
            log_reader.kill().await?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the control pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod-control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for two minutes - a reasonable time for vector internals to
            // pick up new `Pod` and collect events from them in idle load.
            // Here, we're assuming that if the `Pod` that was supposed to be
            // ignored was in fact collected (meaning something's wrong with
            // the exclusion logic), we'd see it's data within this time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(120);
            info!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
            info!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    assert!(got_control_marker);

    drop(excluded_test_pod);
    drop(control_test_pod);
    drop(affinity_pod);
    drop(affinity_ns);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly filters out the logs by the
/// custom selectors, based on k8s API `Pod` labels and annotations.
#[tokio::test]
async fn custom_selectors() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    const CONFIG: &str = indoc! {r#"
        role: "Agent"
        customConfig:
          data_dir: "/vector-data-dir"
          api:
            enabled: true
            address: 127.0.0.1:8686
          sources:
            kubernetes_logs:
              type: kubernetes_logs
              extra_label_selector: "my_custom_negative_label_selector!=my_val"
              extra_namespace_label_selector: "my_custom_negative_label_selector!=my_val"
              extra_field_selector: "metadata.name!=test-pod-excluded-by-name"
          sinks:
            stdout:
              type: console
              inputs: [kubernetes_logs]
              encoding:
                codec: json
                json:
                    pretty: false
    "#};

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), CONFIG],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let label_sets = vec![
        ("test-pod-excluded-1", vec![("vector.dev/exclude", "true")]),
        (
            "test-pod-excluded-2",
            vec![("my_custom_negative_label_selector", "my_val")],
        ),
        ("test-pod-excluded-by-name", vec![]),
    ];
    let mut excluded_test_pods = Vec::new();
    let mut excluded_test_pod_names = Vec::new();
    for (name, label_set) in label_sets {
        excluded_test_pods.push(
            framework
                .test_pod(test_pod::Config::from_pod(&make_test_pod(
                    &pod_namespace,
                    name,
                    "echo EXCLUDED_MARKER",
                    label_set,
                    vec![],
                ))?)
                .await?,
        );
        excluded_test_pod_names.push(name);
    }
    for name in excluded_test_pod_names {
        let name = format!("pods/{}", name);
        framework
            .wait(
                &pod_namespace,
                vec![name.as_ref()],
                WaitFor::Condition("initialized"),
                vec!["--timeout=60s"],
            )
            .await?;
    }

    let control_test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod-control",
            "echo CONTROL_MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod-control"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(
            &pod_namespace,
            "test-pod-control",
            &namespace,
            &override_name,
        )
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill().await?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        debug!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up == 0 {
            info!("Giving up");
            log_reader.kill().await?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the control pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod-control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for two minutes - a reasonable time for vector internals to
            // pick up new `Pod` and collect events from them in idle load.
            // Here, we're assuming that if the `Pod` that was supposed to be
            // ignored was in fact collected (meaning something's wrong with
            // the exclusion logic), we'd see it's data within this time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(120);
            info!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
            info!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    assert!(got_control_marker);

    drop(excluded_test_pods);
    drop(control_test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly filters out the logs from
/// particular containers that are requested to be excluded from collection,
/// based on k8s API `Pod` annotations.
#[tokio::test]
async fn container_filtering() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod_with_containers(
            &pod_namespace,
            "test-pod",
            vec![],
            vec![("vector.dev/exclude-containers", "excluded")],
            None,
            vec![
                make_test_container("excluded", "echo EXCLUDED_MARKER"),
                make_test_container("control", "echo CONTROL_MARKER"),
            ],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill().await?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        debug!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up == 0 {
            info!("Giving up");
            log_reader.kill().await?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the test pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod");

        // Ensure we got the log event from the control container.
        assert_eq!(val["kubernetes"]["container_name"], "control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for 30 seconds - a reasonable time for vector internals to
            // ingest logs for each container in a `Pod` in idle load.
            // Here, we're assuming that if the container log file that was
            // supposed to be ignored was in fact collected (meaning something's
            // wrong with the exclusion logic), we'd see it's data within this
            // time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(30);
            info!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
            info!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    assert!(got_control_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly filters out the logs matching
/// the exclusion glob patterns specified at the `kubernetes_logs`
/// configuration.
#[tokio::test]
async fn glob_pattern_filtering() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    const CONFIG: &str = indoc! {r#"
        role: "Agent"
        customConfig:
          data_dir: "/vector-data-dir"
          api:
            enabled: true
            address: 127.0.0.1:8686
          sources:
            kubernetes_logs:
              type: kubernetes_logs
              exclude_paths_glob_patterns: ["/var/log/pods/*_test-pod_*/excluded/**"]
          sinks:
            stdout:
              type: console
              inputs: [kubernetes_logs]
              encoding:
                codec: json
                json:
                    pretty: false
    "#};

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), CONFIG],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod_with_containers(
            &pod_namespace,
            "test-pod",
            vec![],
            vec![],
            None,
            vec![
                make_test_container("excluded", "echo EXCLUDED_MARKER"),
                make_test_container("control", "echo CONTROL_MARKER"),
            ],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the log lines until the reasonable amount of time passes for us
    // to be confident that vector should've picked up the excluded message
    // if it wasn't filtering it.
    let mut got_control_marker = false;
    let mut lines_till_we_give_up: usize = 10000;
    let (stop_tx, mut stop_rx) = futures::channel::mpsc::channel(0);
    loop {
        let line = tokio::select! {
            result = stop_rx.next() => {
                result.unwrap();
                log_reader.kill().await?;
                continue;
            }
            line = log_reader.read_line() => line,
        };
        let line = match line {
            Some(line) => line,
            None => break,
        };
        debug!("Got line: {:?}", line);

        lines_till_we_give_up -= 1;
        if lines_till_we_give_up == 0 {
            info!("Giving up");
            log_reader.kill().await?;
            break;
        }

        if !line.starts_with('{') {
            // This isn't a json, must be an entry from Vector's own log stream.
            continue;
        }

        let val = parse_json(&line)?;

        if val["kubernetes"]["pod_namespace"] != pod_namespace.as_str() {
            // A log from something other than our test pod, pretend we don't
            // see it.
            continue;
        }

        // Ensure we got the log event from the test pod.
        assert_eq!(val["kubernetes"]["pod_name"], "test-pod");

        // Ensure we got the log event from the control container.
        assert_eq!(val["kubernetes"]["container_name"], "control");

        // Ensure the test sanity by validating that we got the control marker.
        // If we get an excluded marker here - it's an error.
        assert_eq!(val["message"], "CONTROL_MARKER");

        if got_control_marker {
            // We've already seen one control marker! This is not good, we only
            // emitted one.
            panic!("Control marker seen more than once");
        }

        // Remember that we've seen a control marker.
        got_control_marker = true;

        // Request termination in a while.
        let mut stop_tx = stop_tx.clone();
        tokio::spawn(async move {
            // Wait for 30 seconds - a reasonable time for vector internals to
            // ingest logs for each log file of a `Pod` in idle load.
            // Here, we're assuming that if the log file that was supposed to be
            // ignored was in fact collected (meaning something's wrong with
            // the exclusion logic), we'd see it's data within this time frame.
            // It's not enough to just wait for `Pod` complete, we should still
            // apply a reasonably big timeout before we stop waiting for the
            // logs to appear to have high confidence that Vector has enough
            // time to pick them up and spit them out.
            let duration = std::time::Duration::from_secs(30);
            info!("Starting stop timer, due in {} seconds", duration.as_secs());
            tokio::time::sleep(duration).await;
            info!("Stop timer complete");
            stop_tx.send(()).await.unwrap();
        });
    }

    // Ensure log reader exited.
    log_reader.wait().await.expect("log reader wait failed");

    assert!(got_control_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly collects logs from multiple
/// `Namespace`s and `Pod`s.
#[tokio::test]
async fn multiple_ns() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let affinity_label = format!("{}-affinity", pod_namespace);
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    let mut test_namespaces = vec![];
    let mut expected_namespaces = HashSet::new();
    for i in 0..10 {
        let name = format!("{}-{}", pod_namespace, i);
        test_namespaces.push(
            framework
                .namespace(namespace::Config::from_namespace(
                    &namespace::make_namespace(name.clone(), None),
                )?)
                .await?,
        );
        expected_namespaces.insert(name);
    }

    // Create a pod for our other pods to have an affinity to ensure they are all deployed on
    // the same node.
    let affinity_ns_name = format!("{}-affinity", pod_namespace);
    let affinity_ns = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(affinity_ns_name.clone(), None),
        )?)
        .await?;
    let affinity_pod = create_affinity_pod(&framework, &affinity_ns_name, &affinity_label).await?;

    let mut test_pods = vec![];
    for ns in &expected_namespaces {
        debug!("creating {}", ns);
        let test_pod = framework
            .test_pod(test_pod::Config::from_pod(&make_test_pod_with_affinity(
                ns,
                "test-pod",
                "echo MARKER",
                vec![],
                vec![],
                Some((affinity_label.as_str(), "yes")),
                Some(&affinity_ns_name),
            ))?)
            .await?;
        framework
            .wait(
                ns,
                vec!["pods/test-pod"],
                WaitFor::Condition("initialized"),
                vec!["--timeout=60s"],
            )
            .await?;
        test_pods.push(test_pod);
    }

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(
            &affinity_ns_name,
            "affinity-pod",
            &namespace,
            &override_name,
        )
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    look_for_log_line(&mut log_reader, |val| {
        let ns = match val["kubernetes"]["pod_namespace"].as_str() {
            Some(val) if val.starts_with(&pod_namespace) => val,
            _ => {
                // A log from something other than our test pod, pretend we
                // don't see it.
                return FlowControlCommand::GoOn;
            }
        };

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        // Remove the namespace from the list of namespaces we still expect to
        // get.
        let as_expected = expected_namespaces.remove(ns);
        assert!(as_expected);

        if expected_namespaces.is_empty() {
            // We got all the messages we expected, request to stop the flow.
            FlowControlCommand::Terminate
        } else {
            // We didn't get all the messages yet.
            FlowControlCommand::GoOn
        }
    })
    .await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    // Ensure that we have collected messages from all the namespaces.
    assert!(expected_namespaces.is_empty());

    drop(affinity_pod);
    drop(affinity_ns);
    drop(test_pods);
    drop(test_namespaces);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector helm chart properly allows
/// configuration via an additional config file, i.e. it can combine the managed
/// and custom config files.
#[tokio::test]
async fn existing_config_file() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_EXISTING_CONFIGMAP,
                ],
                custom_resource: CUSTOM_RESOURCE_VECTOR_CONFIG,
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}

/// This test validates that vector properly exposes metrics in
/// a Prometheus scraping format.
#[tokio::test]
async fn metrics_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait until `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    // We want to capture the initial value for the `processed_events` metric,
    // but until the `kubernetes_logs` source loads the `Pod`s list, it's
    // internal file server discovers the log files, and some events get
    // a chance to be processed - we don't have a reason to believe that
    // the `processed_events` is even defined.
    // We give Vector some reasonable time to perform this initial bootstrap,
    // and capture the `processed_events` value afterwards.
    debug!("Waiting for Vector bootstrap");
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    debug!("Done waiting for Vector bootstrap");

    // Capture events processed before deploying the test pod.
    let processed_events_before =
        metrics::get_component_sent_events_total(&vector_metrics_url).await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            &pod_namespace,
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Make sure we read the correct nodes logs.
    let vector_pod = framework
        .get_vector_pod_with_pod(&pod_namespace, "test-pod", &namespace, &override_name)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("pod/{}", vector_pod))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != pod_namespace {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    // Due to how `internal_metrics` are implemented, we have to wait for it's
    // scraping period to pass before we can observe the updates.
    debug!("Waiting for `internal_metrics` to update");
    tokio::time::sleep(std::time::Duration::from_secs(6)).await;
    debug!("Done waiting for `internal_metrics` to update");

    // Capture events processed after the test pod has finished.
    let processed_events_after =
        metrics::get_component_sent_events_total(&vector_metrics_url).await?;

    // Ensure we did get at least one event since before deployed the test pod.
    assert!(
        processed_events_after > processed_events_before,
        "before: {}, after: {}",
        processed_events_before,
        processed_events_after
    );

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    drop(test_pod);
    drop(test_namespace);
    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

/// This test validates that vector chart properly exposes host metrics
/// out of the box.
#[tokio::test]
async fn host_metrics() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&override_name, true),
                    HELM_VALUES_AGENT,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{}", override_name),
        9090,
        9090,
    )?;
    vector_metrics_port_forward.wait_until_ready().await?;
    let vector_metrics_url = format!(
        "http://{}/metrics",
        vector_metrics_port_forward.local_addr_ipv4()
    );

    // Wait that `vector_started`-ish metric is present.
    metrics::wait_for_vector_started(
        &vector_metrics_url,
        std::time::Duration::from_secs(5),
        std::time::Instant::now() + std::time::Duration::from_secs(60),
    )
    .await?;

    // We want to capture the value for the host metrics, but the pipeline for
    // collecting them takes some time to boot (15s roughly).
    // We wait twice as much, so the bootstrap is guaranteed.
    debug!("Waiting for Vector bootstrap");
    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
    debug!("Done waiting for Vector bootstrap");

    // Ensure the host metrics are exposed in the Prometheus endpoint.
    metrics::assert_metrics_present(&vector_metrics_url, metrics::HOST_METRICS).await?;

    metrics::assert_metrics_present(&vector_metrics_url, metrics::SOURCE_COMPLIANCE_METRICS)
        .await?;

    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}

#[tokio::test]
async fn simple_checkpoint() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .helm_chart(
            "test-vector",
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![HELM_VALUES_AGENT],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout("test-vector", "daemonset/vector", vec!["--timeout=60s"])
        .await?;

    let test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace("test-vector-test-pod".to_string(), None),
        )?)
        .await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            // This allows us to read and checkpoint the first log
            // then ensure we just read the new marker after restarting Vector
            "echo CHECKED_MARKER; sleep 90; echo MARKER",
            vec![],
            vec![],
        ))?)
        .await?;
    framework
        .wait(
            "test-vector-test-pod",
            vec!["pods/test-pod"],
            WaitFor::Condition("initialized"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut log_reader = framework.logs("test-vector", "daemonset/vector")?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            // A log from something other than our test pod, pretend we don't
            // see it.
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "CHECKED_MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;
    assert!(got_marker);

    framework
        .restart_rollout("test-vector", "daemonset/vector", vec![])
        .await?;
    // We need to wait for the new pod to start
    framework
        .wait_for_rollout("test-vector", "daemonset/vector", vec!["--timeout=60s"])
        .await?;
    got_marker = false;
    // We need to start reading from the newly started pod
    let mut log_reader = framework.logs("test-vector", "daemonset/vector")?;
    look_for_log_line(&mut log_reader, |val| {
        if val["kubernetes"]["pod_namespace"] != "test-vector-test-pod" {
            return FlowControlCommand::GoOn;
        }

        if val["message"].eq("CHECKED_MARKER") {
            panic!("Checkpointed marker should not be found");
        };

        assert_eq!(val["message"], "MARKER");

        if got_marker {
            // We've already seen one marker! This is not good, we only emitted
            // one.
            panic!("Marker seen more than once");
        }

        // If we did, remember it.
        got_marker = true;

        // Request to stop the flow.
        FlowControlCommand::Terminate
    })
    .await?;

    assert!(got_marker);

    drop(test_pod);
    drop(test_namespace);
    drop(vector);
    Ok(())
}
