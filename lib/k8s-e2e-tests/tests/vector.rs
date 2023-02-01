use indoc::{formatdoc, indoc};
use k8s_e2e_tests::*;
use k8s_test_framework::{
    lock, namespace, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};

fn helm_values_stdout_sink(agent_override_name: &str) -> String {
    formatdoc!(
        r#"
    role: Agent
    fullnameOverride: "{}"
    env:
    - name: VECTOR_REQUIRE_HEALTHY
      value: true
    customConfig:
      data_dir: "/vector-data-dir"
      api:
        enabled: true
        address: 127.0.0.1:8686
      sources:
        kubernetes_logs:
          type: kubernetes_logs
      sinks:
        vector:
          type: vector
          inputs: [kubernetes_logs]
          address: aggregator-vector:6000
          version: "2"
    "#,
        agent_override_name,
    )
}

fn helm_values_haproxy(agent_override_name: &str) -> String {
    formatdoc!(
        r#"
    role: Agent
    fullnameOverride: "{}"
    env:
    - name: VECTOR_REQUIRE_HEALTHY
      value: true
    customConfig:
      data_dir: "/vector-data-dir"
      api:
        enabled: true
        address: 127.0.0.1:8686
      sources:
        kubernetes_logs:
          type: kubernetes_logs
      sinks:
        vector:
          type: vector
          inputs: [kubernetes_logs]
          address: aggregator-vector-haproxy:6000
          version: "2"
    "#,
        agent_override_name,
    )
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
    let agent_override_name = get_override_name(&namespace, "vector-agent");

    let vector_aggregator = framework
        .helm_chart(
            &namespace,
            "vector",
            "aggregator",
            "https://helm.vector.dev",
            VectorConfig {
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("statefulset/aggregator-vector"),
            vec!["--timeout=60s"],
        )
        .await?;

    let vector_agent = framework
        .helm_chart(
            &namespace,
            "vector",
            "agent",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&helm_values_stdout_sink(&agent_override_name)],
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

    let mut log_reader = framework.logs(&namespace, &format!("statefulset/aggregator-vector"))?;
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
    drop(vector_agent);
    drop(vector_aggregator);
    Ok(())
}

/// This test validates that vector picks up logs with an agent and
/// delivers them to the aggregator through an HAProxy load balancer.
#[tokio::test]
async fn haproxy() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();
    let agent_override_name = get_override_name(&namespace, "vector-agent");

    const CONFIG: &str = indoc! {r#"
        haproxy:
          enabled: true
    "#};

    let vector_aggregator = framework
        .helm_chart(
            &namespace,
            "vector",
            "aggregator",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![CONFIG],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("statefulset/aggregator-vector"),
            vec!["--timeout=60s"],
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("deployment/aggregator-vector-haproxy"),
            vec!["--timeout=60s"],
        )
        .await?;

    let vector_agent = framework
        .helm_chart(
            &namespace,
            "vector",
            "agent",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&helm_values_haproxy(&agent_override_name)],
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

    let mut log_reader = framework.logs(&namespace, &format!("statefulset/aggregator-vector"))?;
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
    drop(vector_agent);
    drop(vector_aggregator);
    Ok(())
}
