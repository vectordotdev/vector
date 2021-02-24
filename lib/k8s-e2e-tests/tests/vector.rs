use indoc::indoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{
    lock, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};

const HELM_CHART_VECTOR: &str = "vector";

const HELM_VALUES_STDOUT_SINK: &str = indoc! {r#"
    vector-aggregator:
      vectorSource:
        sourceId: vector

      sinks:
        stdout:
          type: "console"
          inputs: ["vector"]
          target: "stdout"
          encoding: "json"
"#};

/// This test validates that vector picks up logs with an agent and
/// delivers them to the aggregator out of the box.
#[tokio::test]
async fn logs() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    let framework = make_framework();

    let vector = framework
        .vector(
            "test-vector",
            HELM_CHART_VECTOR,
            VectorConfig {
                custom_helm_values: HELM_VALUES_STDOUT_SINK,
                ..Default::default()
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "daemonset/vector-agent",
            vec!["--timeout=60s"],
        )
        .await?;
    framework
        .wait_for_rollout(
            "test-vector",
            "statefulset/vector-aggregator",
            vec!["--timeout=60s"],
        )
        .await?;

    let test_namespace = framework.namespace("test-vector-test-pod").await?;

    let test_pod = framework
        .test_pod(test_pod::Config::from_pod(&make_test_pod(
            "test-vector-test-pod",
            "test-pod",
            "echo MARKER",
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

    let mut log_reader = framework.logs("test-vector", "statefulset/vector-aggregator")?;
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
