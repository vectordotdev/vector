use indoc::formatdoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{
    lock, namespace, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};

fn helm_values_stdout_sink(aggregator_override_name: &str, agent_override_name: &str) -> String {
    if is_multinode() {
        formatdoc!(
            r#"
    global:
      vector:
        commonEnvKV:
          VECTOR_REQUIRE_HEALTHY: true

    vector-agent:
      fullnameOverride: "{}"
      kubernetesLogsSource:
        rawConfig: |
          glob_minimum_cooldown_ms = 5000
      vectorSink:
        host: "{}"
      dataVolume:
        hostPath:
          path: /var/lib/{}-vector/

      extraVolumeMounts:
        - name: var-lib
          mountPath: /var/writablelib
          readOnly: false

      lifecycle:
        preStop:
          exec:
            command:
              - sh
              - -c
              - rm -rf /var/writablelib/{}-vector

    vector-aggregator:
      fullnameOverride: "{}"
      vectorSource:
        sourceId: vector

      sinks:
        stdout:
          type: "console"
          inputs: ["vector"]
          target: "stdout"
          encoding: "json"
    "#,
            agent_override_name,
            aggregator_override_name,
            agent_override_name,
            agent_override_name,
            aggregator_override_name
        )
    } else {
        formatdoc!(
            r#"
    global:
      vector:
        commonEnvKV:
          VECTOR_REQUIRE_HEALTHY: true

    vector-agent:
      fullnameOverride: "{}"
      kubernetesLogsSource:
        rawConfig: |
          glob_minimum_cooldown_ms = 5000
      vectorSink:
        host: "{}"

    vector-aggregator:
      fullnameOverride: "{}"
      vectorSource:
        sourceId: vector

      sinks:
        stdout:
          type: "console"
          inputs: ["vector"]
          target: "stdout"
          encoding: "json"
    "#,
            agent_override_name,
            aggregator_override_name,
            aggregator_override_name
        )
    }
}

fn helm_values_haproxy(aggregator_override_name: &str, agent_override_name: &str) -> String {
    if is_multinode() {
        formatdoc!(
            r#"
    global:
      vector:
        commonEnvKV:
          VECTOR_REQUIRE_HEALTHY: true

    vector-agent:
      fullnameOverride: "{}"
      kubernetesLogsSource:
        rawConfig: |
          glob_minimum_cooldown_ms = 5000
      vectorSink:
        host: "{}-haproxy"
      dataVolume:
        hostPath:
          path: /var/lib/{}-vector/

    vector-aggregator:
      fullnameOverride: "{}"
      vectorSource:
        sourceId: vector

      sinks:
        stdout:
          type: "console"
          inputs: ["vector"]
          target: "stdout"
          encoding: "json"

      haproxy:
        enabled: true
    "#,
            agent_override_name,
            aggregator_override_name,
            agent_override_name,
            aggregator_override_name
        )
    } else {
        formatdoc!(
            r#"
    global:
      vector:
        commonEnvKV:
          VECTOR_REQUIRE_HEALTHY: true

    vector-agent:
      fullnameOverride: "{}"
      vectorSink:
        host: "{}-haproxy"

    vector-aggregator:
      fullnameOverride: "{}"
      vectorSource:
        sourceId: vector

      sinks:
        stdout:
          type: "console"
          inputs: ["vector"]
          target: "stdout"
          encoding: "json"

      haproxy:
        enabled: true
    "#,
            agent_override_name,
            aggregator_override_name,
            aggregator_override_name
        )
    }
}

/// This test validates that vector picks up logs with an agent and
/// delivers them to the aggregator out of the box.
#[tokio::test]
async fn logs() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let aggregator_override_name = get_override_name(&namespace, "vector-aggregator");
    let agent_override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "https://packages.timber.io/helm/nightly/",
            VectorConfig {
                custom_helm_values: vec![&helm_values_stdout_sink(
                    &aggregator_override_name,
                    &agent_override_name,
                )],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", agent_override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("statefulset/{}", aggregator_override_name),
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

    let mut log_reader = framework.logs(
        &namespace,
        &format!("statefulset/{}", aggregator_override_name),
    )?;
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

/// This test validates that vector picks up logs with an agent and
/// delivers them to the aggregator through an HAProxy load balancer.
#[tokio::test]
async fn logs_haproxy() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let aggregator_override_name = get_override_name(&namespace, "vector-aggregator");
    let agent_override_name = get_override_name(&namespace, "vector-agent");

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "https://packages.timber.io/helm/nightly/",
            VectorConfig {
                custom_helm_values: vec![&helm_values_haproxy(
                    &aggregator_override_name,
                    &agent_override_name,
                )],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{}", agent_override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("statefulset/{}", aggregator_override_name),
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

    let mut log_reader = framework.logs(
        &namespace,
        &format!("statefulset/{}", aggregator_override_name),
    )?;
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
