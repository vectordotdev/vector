use indoc::indoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{lock, namespace, test_pod, vector::Config as VectorConfig};
use serde_json::Value;

/// This test validates that vector-aggregator can deploy with the default
/// settings and a dummy topology.
#[tokio::test]
async fn datadog_to_vector() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let namespace = get_namespace();
    let override_name = get_override_name(&namespace, "vector-aggregator");
    let vector_endpoint = &format!("{}.{}.svc.cluster.local", override_name, namespace);
    let datadog_namespace = get_namespace_appended(&namespace, "datadog-agent");
    let datadog_override_name = get_override_name(&namespace, "datadog-agent");
    let pod_namespace = get_namespace_appended(&namespace, "test-pod");
    let framework = make_framework();

    // Value.yaml for datadog official chart
    let datadog_chart_values = &format!(
        indoc! {r#"
        datadog:
          apiKey: 0123456789ABCDEF0123456789ABCDEF
          logs:
            enabled: true
          processAgent:
            enabled: false
          clusterAgent:
            enabled: false
          kubeStateMetricsEnabled: false
        agents:
          containers:
            agent:
              readinessProbe:
                exec:
                  command: ["/bin/true"]
          useConfigMap: true
          customAgentConfig:
            kubelet_tls_verify: false
            logs_config.use_http: true
            logs_config.logs_no_ssl: true
            logs_config.logs_dd_url: {}:8282
            listeners:
              - name: kubelet
            config_providers:
              - name: kubelet
                polling: true
              - name: docker
                polling: true
"#},
        vector_endpoint
    );

    let _vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, false)],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("statefulset/{}", override_name),
            vec!["--timeout=60s"],
        )
        .await?;

    let _datadog_agent = framework
        .helm_chart(
            &datadog_namespace,
            "datadog",
            "datadog",
            "https://helm.datadoghq.com",
            // VectorConfig is a generic config container
            VectorConfig {
                custom_helm_values: vec![
                    &config_override_name(&datadog_override_name, false),
                    datadog_chart_values,
                ],
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &datadog_namespace,
            &format!("daemonset/{}", datadog_override_name),
            vec!["--timeout=60s"],
        )
        .await?;
    let _test_namespace = framework
        .namespace(namespace::Config::from_namespace(
            &namespace::make_namespace(pod_namespace.clone(), None),
        )?)
        .await?;

    let _test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![],
            // Annotation to enable log collection by the Datadog agent
            vec![(
                "ad.datadoghq.com/test-pod.logs",
                "[{\"source\":\"test_source\",\"service\":\"test_service\"}]",
            )],
        ))?)
        .await?;

    let mut log_reader = framework.logs(&namespace, &format!("statefulset/{}", override_name))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if val["service"] != Value::Null && val["service"] != "test_service" {
            panic!("Unexpected logs");
        } else if val["service"] == Value::Null {
            return FlowControlCommand::GoOn;
        }

        // Ensure we got the marker.
        assert_eq!(val["message"], "MARKER");
        assert_eq!(val["source_type"], "datadog_agent");

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

    Ok(())
}
