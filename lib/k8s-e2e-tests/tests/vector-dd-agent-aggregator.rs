use indoc::indoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{lock, test_pod, vector::Config as VectorConfig};


const HELM_CHART_VECTOR_AGGREGATOR: &str = "vector-aggregator";

const HELM_VALUES_DDOG_AGG_TOPOLOGY: &str = indoc! {r#"
    sources:
      ddog-agg:
        type: "datadog-agent"
        rawConfig: |
          address = "0.0.0.0:8080"

    sinks:
      stdout:
        type: "console"
        inputs: ["datadog-agent"]
        target: "stdout"
        encoding: "json"
"#};

// Value.yaml for datadog offical chart
const DATADOG_AGENT_CHART_CONFIG: &str = indoc! {r#"
    datadog:
      apiKey: 0123456789ABCDEF0123456789ABCDEF
      logs:
        enabled: true
    agents:
      customAgentConfig:
        kubelet_tls_verify: false
        logs_config.use_http: true
        logs_config.logs_no_ssl: true
        logs_config.logs_dd_url: vector-aggregator:8080
        listeners:
          - name: kubelet
        config_providers:
          - name: kubelet
            polling: true
          - name: docker
            polling: true
"#};


/// This test validates that vector-aggregator can deploy with the default
/// settings and a dummy topology.
#[tokio::test]
async fn datadog_to_vector() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let namespace = get_namespace();
    let framework = make_framework();
    let pod_namespace = get_namespace_appended("test-pod");
    let override_name = get_override_name("vector-aggregator");

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
            &namespace,
            "datadog",
            "https://helm.datadoghq.com",
            // Vector is a generic config container
            VectorConfig {
                custom_helm_values: DATADOG_AGENT_CHART_CONFIG,
                ..Default::default()
            }
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/datadog-agent", ),
            vec!["--timeout=60s"],
        )
        .await?;

        let test_namespace = framework.namespace(&pod_namespace).await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            &pod_namespace,
            "test-pod",
            "echo MARKER",
            vec![("ad.datadoghq.com/test-pod.logs", "[{\"source\":\"test_source\",\"service\":\"test_service\"}]")],
            vec![],
        ))?)
        .await?;

    let mut log_reader =
        framework.logs(&namespace, &format!("statefulset/{}", "vector-aggregator"))?;
    smoke_check_first_line(&mut log_reader).await;

    // Read the rest of the log lines.
    let mut got_marker = false;
    look_for_log_line(&mut log_reader, |val| {
        if let Some(service) = val.get("service") {
            if service != json!("test_service") {
                fewfew
            }
        }
        else {
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
    drop(datadog_agent);
    drop(vector);
    Ok(())
}