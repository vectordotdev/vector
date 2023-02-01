use k8s_e2e_tests::*;
use k8s_test_framework::{lock, vector::Config as VectorConfig};

/// This test validates that vector can deploy with the default
/// aggregator settings.
#[tokio::test]
async fn dummy_topology() -> Result<(), Box<dyn std::error::Error>> {
    init();

    let _guard = lock();
    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-aggregator");

    let vector = framework
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

    drop(vector);
    Ok(())
}

/// This test validates that vector-aggregator chart properly exposes metrics in
/// a Prometheus scraping format ot of the box.
#[tokio::test]
async fn metrics_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    init();

    let _guard = lock();
    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-aggregator");

    let vector = framework
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

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("statefulset/{}", override_name),
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

    drop(vector_metrics_port_forward);
    drop(vector);
    Ok(())
}
