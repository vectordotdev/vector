#![allow(clippy::await_holding_lock)]

use std::process::Stdio;
use std::time::{Duration, Instant};

use chrono::{SecondsFormat, Utc};
use indoc::formatdoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{lock, vector::Config as VectorConfig};
use tokio::io::AsyncWriteExt;

const KUBERNETES_EVENTS_LEADER_ELECTION_REASON: &str = "VectorLeaderElectionTest";

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
            &format!("statefulset/{override_name}"),
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
            &format!("statefulset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    let mut vector_metrics_port_forward = framework.port_forward(
        &namespace,
        &format!("statefulset/{override_name}"),
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

/// This test validates that the kubernetes_events source emits events from only
/// one replica when Lease-based leader election is enabled, and continues after
/// the leader pod is removed.
#[tokio::test]
async fn kubernetes_events_leader_election() -> Result<(), Box<dyn std::error::Error>> {
    init();

    let _guard = lock();
    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector-events-leader");
    let lease_name = format!("{override_name}-events");
    let helm_values =
        kubernetes_events_leader_election_values(&override_name, &namespace, &lease_name);
    let rbac = kubernetes_events_leader_election_rbac(&override_name);

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            "https://helm.vector.dev",
            VectorConfig {
                custom_helm_values: vec![&helm_values],
                custom_resource: &rbac,
            },
        )
        .await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("deployment/{override_name}"),
            vec!["--timeout=90s"],
        )
        .await?;

    let first_event = format!("{override_name}-first");
    apply_test_event(&namespace, &first_event).await?;
    wait_for_event_count(&namespace, &override_name, &first_event, 1).await?;

    let first_holder = wait_for_lease_holder(&namespace, &lease_name, None).await?;
    kubectl(&["delete", "pod", "-n", &namespace, &first_holder]).await?;
    framework
        .wait_for_rollout(
            &namespace,
            &format!("deployment/{override_name}"),
            vec!["--timeout=90s"],
        )
        .await?;
    let _second_holder =
        wait_for_lease_holder(&namespace, &lease_name, Some(&first_holder)).await?;

    let second_event = format!("{override_name}-second");
    apply_test_event(&namespace, &second_event).await?;
    wait_for_event_count(&namespace, &override_name, &second_event, 1).await?;

    drop(vector);
    Ok(())
}

fn kubernetes_events_leader_election_values(
    override_name: &str,
    namespace: &str,
    lease_name: &str,
) -> String {
    formatdoc! {r#"
        role: "Stateless-Aggregator"
        fullnameOverride: "{override_name}"
        replicas: 2
        image:
          pullPolicy: IfNotPresent
        service:
          enabled: false
        serviceHeadless:
          enabled: false
        customConfig:
          data_dir: /vector-data-dir
          sources:
            kubernetes_events:
              type: kubernetes_events
              namespaces: ["{namespace}"]
              include_reasons: ["{KUBERNETES_EVENTS_LEADER_ELECTION_REASON}"]
              leader_election:
                enabled: true
                lease_name: "{lease_name}"
                lease_namespace: "{namespace}"
                identity_env_var: HOSTNAME
                lease_duration_seconds: 8
                renew_deadline_seconds: 5
                retry_period_seconds: 1
          sinks:
            stdout:
              type: console
              inputs: [kubernetes_events]
              encoding:
                codec: json
    "#}
}

fn kubernetes_events_leader_election_rbac(override_name: &str) -> String {
    formatdoc! {r#"
        apiVersion: rbac.authorization.k8s.io/v1
        kind: Role
        metadata:
          name: {override_name}-events
        rules:
          - apiGroups: ["events.k8s.io"]
            resources: ["events"]
            verbs: ["get", "list", "watch"]
          - apiGroups: ["coordination.k8s.io"]
            resources: ["leases"]
            verbs: ["get", "create", "update"]
        ---
        apiVersion: rbac.authorization.k8s.io/v1
        kind: RoleBinding
        metadata:
          name: {override_name}-events
        subjects:
          - kind: ServiceAccount
            name: {override_name}
        roleRef:
          apiGroup: rbac.authorization.k8s.io
          kind: Role
          name: {override_name}-events
    "#}
}

async fn apply_test_event(
    namespace: &str,
    event_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_time = Utc::now().to_rfc3339_opts(SecondsFormat::Micros, true);
    let manifest = formatdoc! {r#"
        apiVersion: events.k8s.io/v1
        kind: Event
        metadata:
          name: {event_name}
          namespace: {namespace}
        eventTime: "{event_time}"
        action: Testing
        reportingController: vector.dev/e2e
        reportingInstance: {event_name}
        reason: {KUBERNETES_EVENTS_LEADER_ELECTION_REASON}
        regarding:
          apiVersion: v1
          kind: Pod
          name: leader-election-test
          namespace: {namespace}
        note: "{event_name}"
        type: Normal
    "#};

    kubectl_stdin(&["apply", "-f", "-"], manifest.as_bytes()).await
}

async fn wait_for_event_count(
    namespace: &str,
    pod_name_prefix: &str,
    event_name: &str,
    expected_count: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(60);

    loop {
        let count = count_event_logs(namespace, pod_name_prefix, event_name).await?;
        if count == expected_count {
            return Ok(());
        }
        if count > expected_count {
            return Err(format!(
                "expected {expected_count} log line(s) for event {event_name}, found {count}"
            )
            .into());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for {expected_count} log line(s) for event {event_name}, found {count}"
            )
            .into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn count_event_logs(
    namespace: &str,
    pod_name_prefix: &str,
    event_name: &str,
) -> Result<usize, Box<dyn std::error::Error>> {
    let pods = pods_with_prefix(namespace, pod_name_prefix).await?;
    let mut count = 0;
    for pod in pods {
        let output = kubectl(&["logs", "-n", namespace, &pod]).await?;
        count += output
            .lines()
            .filter(|line| line.contains(event_name))
            .count();
    }
    Ok(count)
}

async fn wait_for_lease_holder(
    namespace: &str,
    lease_name: &str,
    previous_holder: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(60);

    loop {
        let holder = kubectl(&[
            "get",
            "lease",
            "-n",
            namespace,
            lease_name,
            "-o",
            "jsonpath={.spec.holderIdentity}",
        ])
        .await?
        .trim()
        .to_string();

        if !holder.is_empty() && previous_holder.is_none_or(|previous| previous != holder) {
            return Ok(holder);
        }
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for lease holder on {lease_name}").into());
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn pods_with_prefix(
    namespace: &str,
    pod_name_prefix: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let output = kubectl(&["get", "pods", "-n", namespace, "-o", "json"]).await?;
    let pods: serde_json::Value = serde_json::from_str(&output)?;
    let pods = pods["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pod| pod["metadata"]["name"].as_str())
        .filter(|name| name.starts_with(pod_name_prefix))
        .map(ToString::to_string)
        .collect();

    Ok(pods)
}

async fn kubectl(args: &[&str]) -> Result<String, Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new(kubectl_bin())
        .args(args)
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "kubectl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(String::from_utf8(output.stdout)?)
}

async fn kubectl_stdin(args: &[&str], stdin: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut child = tokio::process::Command::new(kubectl_bin())
        .args(args)
        .stdin(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .take()
        .expect("stdin should be piped")
        .write_all(stdin)
        .await?;

    let output = child.wait_with_output().await?;
    if !output.status.success() {
        return Err(format!(
            "kubectl {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(())
}

fn kubectl_bin() -> String {
    std::env::var("VECTOR_TEST_KUBECTL").unwrap_or_else(|_| "kubectl".to_string())
}
