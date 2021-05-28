use indoc::indoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{lock, test_pod, vector::Config as VectorConfig};
use serde_json::Value;

const HELM_CHART_VECTOR_AGGREGATOR: &str = "vector-aggregator";

const HELM_VALUES_DDOG_AGG_TOPOLOGY: &str = indoc! {r#"
    service:
      type: "ClusterIP"
      ports:
        - name: datadog
          port: 8080
          protocol: TCP
          targetPort: 8080
    sources:
      datadog-agent:
        type: "datadog_logs"
        rawConfig: |
          address = "0.0.0.0:8080"

    sinks:
      stdout:
        type: "console"
        inputs: ["datadog-agent"]
        target: "stdout"
        encoding: "json"
"#};

/// This test validates that vector-aggregator can deploy with the default
/// settings and a dummy topology.
#[tokio::test]
async fn datadog_to_vector() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let namespace = get_namespace();
    let override_name = get_override_name("vector-aggregator");
    let vector_endpoint = &format!("{}.{}.svc.cluster.local", override_name, namespace);
    let datadog_namespace = get_namespace_appended("datadog-agent");
    let datadog_override_name = get_override_name("datadog-agent");
    let pod_namespace = get_namespace_appended("test-pod");
    let framework = make_framework();

    // Value.yaml for datadog offical chart
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
            logs_config.logs_dd_url: {}:8080
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

    let vector = framework
        .vector(
            &namespace,
            HELM_CHART_VECTOR_AGGREGATOR,
            VectorConfig {
                custom_helm_values: &config_override_name(
                    HELM_VALUES_DDOG_AGG_TOPOLOGY,
                    &override_name,
                ),
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

    let datadog_agent = framework
        .external_chart(
            &datadog_namespace,
            "datadog",
            "https://helm.datadoghq.com",
            // VectorConfig is a generic config container
            VectorConfig {
                custom_helm_values: &config_override_name(
                    datadog_chart_values,
                    &datadog_override_name,
                ),
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

    let test_namespace = framework.namespace(&pod_namespace).await?;
    let test_pod = framework
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
        assert_eq!(val["source_type"], "datadog_logs");

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
    drop(datadog_agent);
    drop(vector);
    Ok(())
}
