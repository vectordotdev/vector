#![allow(clippy::await_holding_lock)]

use indoc::formatdoc;
use k8s_e2e_tests::*;
use k8s_test_framework::{lock, vector::Config as VectorConfig};
use reqwest::{StatusCode, header};
use tokio::io::AsyncWriteExt;

/// RAII guard for test resources that ensures cleanup on drop.
/// This prevents resource leakage when tests fail before reaching explicit cleanup.
/// Tracks both cluster-scoped RBAC resources and the test namespace.
struct TestResourceCleanup {
    kubectl_command: String,
    namespace: Option<String>,
    resources: Vec<(String, String)>, // (resource_type, name)
}

impl TestResourceCleanup {
    fn new(kubectl_command: String) -> Self {
        Self {
            kubectl_command,
            namespace: None,
            resources: Vec::new(),
        }
    }

    fn track_namespace(&mut self, namespace: String) {
        self.namespace = Some(namespace);
    }

    fn track(&mut self, resource_type: &str, name: String) {
        self.resources.push((resource_type.to_string(), name));
    }
}

impl Drop for TestResourceCleanup {
    fn drop(&mut self) {
        // Best-effort cleanup - run synchronously to ensure completion before test exit
        let kubectl = self.kubectl_command.clone();
        let resources = self.resources.clone();
        let namespace = self.namespace.clone();

        // Join the thread to ensure cleanup completes before Drop returns
        let handle = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                // First delete cluster-scoped RBAC resources
                for (resource_type, name) in resources {
                    let _ = tokio::process::Command::new(&kubectl)
                        .args(["delete", &resource_type, &name, "--ignore-not-found=true"])
                        .output()
                        .await;
                }

                // Then delete the namespace (which also deletes ServiceAccounts in it)
                if let Some(ns) = namespace {
                    let _ = tokio::process::Command::new(&kubectl)
                        .args(["delete", "namespace", &ns, "--ignore-not-found=true"])
                        .output()
                        .await;
                }
            });
        });

        // Wait for cleanup to complete (ignore errors - best effort)
        let _ = handle.join();
    }
}

/// Helper to clean up cluster-scoped resources (ClusterRoles and ClusterRoleBindings)
/// Silently ignores errors (resources may already be deleted)
async fn cleanup_cluster_resources(kubectl_command: &str, resource_names: &[(&str, &str)]) {
    for (resource_type, name) in resource_names {
        let _ = tokio::process::Command::new(kubectl_command)
            .args(["delete", resource_type, name, "--ignore-not-found=true"])
            .output()
            .await;
    }
}

/// Helper to create a namespace using kubectl
async fn create_namespace(
    kubectl_command: &str,
    namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new(kubectl_command)
        .args(["create", "namespace", namespace])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create namespace: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Helper to create a ServiceAccount using kubectl
async fn create_service_account(
    kubectl_command: &str,
    namespace: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new(kubectl_command)
        .args(["create", "serviceaccount", name, "-n", namespace])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create ServiceAccount: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Helper to create a ClusterRole for SAR permissions
async fn create_clusterrole_for_sar(
    kubectl_command: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let yaml = formatdoc!(
        r#"
        apiVersion: rbac.authorization.k8s.io/v1
        kind: ClusterRole
        metadata:
          name: {name}
        rules:
        - apiGroups: ["authentication.k8s.io"]
          resources: ["tokenreviews"]
          verbs: ["create"]
        - apiGroups: ["authorization.k8s.io"]
          resources: ["subjectaccessreviews"]
          verbs: ["create"]
        "#,
        name = name
    );

    let mut child = tokio::process::Command::new(kubectl_command)
        .args(["apply", "-f", "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(yaml.as_bytes())
        .await?;
    let output = child.wait_with_output().await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create ClusterRole: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Helper to create a ClusterRole for metrics access
async fn create_clusterrole_for_metrics(
    kubectl_command: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let yaml = formatdoc!(
        r#"
        apiVersion: rbac.authorization.k8s.io/v1
        kind: ClusterRole
        metadata:
          name: {name}
        rules:
        - nonResourceURLs: ["/metrics"]
          verbs: ["get"]
        "#,
        name = name
    );

    let mut child = tokio::process::Command::new(kubectl_command)
        .args(["apply", "-f", "-"])
        .stdin(std::process::Stdio::piped())
        .spawn()?;

    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(yaml.as_bytes())
        .await?;
    let output = child.wait_with_output().await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create ClusterRole: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Helper to create a ClusterRoleBinding
async fn create_clusterrolebinding(
    kubectl_command: &str,
    name: &str,
    role_name: &str,
    sa_name: &str,
    sa_namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = tokio::process::Command::new(kubectl_command)
        .args([
            "create",
            "clusterrolebinding",
            name,
            "--clusterrole",
            role_name,
            "--serviceaccount",
            &format!("{}:{}", sa_namespace, sa_name),
        ])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create ClusterRoleBinding: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(())
}

/// Helper to get ServiceAccount token using kubectl create token
///
/// This uses `kubectl create token` which works on Kubernetes 1.24+ where
/// legacy token Secrets are no longer automatically created.
async fn get_service_account_token(
    kubectl_command: &str,
    namespace: &str,
    sa_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // Wait a bit for SA to be fully initialized
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let output = tokio::process::Command::new(kubectl_command)
        .args([
            "create",
            "token",
            sa_name,
            "-n",
            namespace,
            "--duration=3600s",
        ])
        .output()
        .await?;

    if !output.status.success() {
        return Err(format!(
            "Failed to create token: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    let token = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(token)
}

/// Test basic SAR authentication with valid token
#[tokio::test]
async fn sar_auth_with_valid_token() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create cleanup guard to ensure RBAC resources are deleted even on test failure
    let mut cleanup = TestResourceCleanup::new(framework.kubectl_command().to_string());

    // Create namespace first
    // Namespace creation
    create_namespace(framework.kubectl_command(), &namespace).await?;
    cleanup.track_namespace(namespace.clone());

    // Create ServiceAccount for Vector with SAR permissions

    create_service_account(framework.kubectl_command(), &namespace, "vector-sa").await?;

    create_clusterrole_for_sar(
        framework.kubectl_command(),
        &format!("{}-sar-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-sar-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    )
    .await?;
    cleanup.track("clusterrolebinding", format!("{}-sar-binding", namespace));

    // Create ServiceAccount for Prometheus scraper with metrics permission

    create_service_account(framework.kubectl_command(), &namespace, "prometheus-sa").await?;

    create_clusterrole_for_metrics(
        framework.kubectl_command(),
        &format!("{}-metrics-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-metrics-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-metrics-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "prometheus-sa",
        &namespace,
    )
    .await?;
    cleanup.track(
        "clusterrolebinding",
        format!("{}-metrics-binding", namespace),
    );

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        role: Agent
        serviceAccount:
          create: false
          name: vector-sa
        customConfig:
          sources:
            internal_metrics:
              type: internal_metrics
          sinks:
            prometheus:
              type: prometheus_exporter
              inputs: [internal_metrics]
              address: "0.0.0.0:9598"
              auth:
                strategy: sar
                path: "/metrics"
                verb: "get"
        service:
          ports:
            - name: metrics
              port: 9598
              protocol: TCP
        "#
    );

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            &helm_chart_repo(),
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), &helm_values],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Port forward to Vector's metrics endpoint
    let mut port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{override_name}"),
        9598,
        9598,
    )?;

    port_forward.wait_until_ready().await?;
    let local_addr = port_forward.local_addr_ipv4();

    // Get Prometheus SA token
    let token =
        get_service_account_token(framework.kubectl_command(), &namespace, "prometheus-sa").await?;

    // Make authenticated request with valid token
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/metrics", local_addr))
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Expected 200 OK with valid Prometheus SA token"
    );

    let body = response.text().await?;
    assert!(
        body.contains("# HELP"),
        "Response should contain Prometheus metrics"
    );

    drop(vector);

    // Cleanup guard will automatically delete cluster-scoped resources on drop

    Ok(())
}

/// Test SAR authentication rejects requests without token
#[tokio::test]
async fn sar_auth_rejects_missing_token() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create cleanup guard to ensure RBAC resources are deleted even on test failure
    let mut cleanup = TestResourceCleanup::new(framework.kubectl_command().to_string());

    // Create namespace first
    // Namespace creation
    create_namespace(framework.kubectl_command(), &namespace).await?;
    cleanup.track_namespace(namespace.clone());

    // Create ServiceAccount for Vector with SAR permissions

    create_service_account(framework.kubectl_command(), &namespace, "vector-sa").await?;

    create_clusterrole_for_sar(
        framework.kubectl_command(),
        &format!("{}-sar-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-sar-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    )
    .await?;
    cleanup.track("clusterrolebinding", format!("{}-sar-binding", namespace));

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        role: Agent
        serviceAccount:
          create: false
          name: vector-sa
        customConfig:
          sources:
            internal_metrics:
              type: internal_metrics
          sinks:
            prometheus:
              type: prometheus_exporter
              inputs: [internal_metrics]
              address: "0.0.0.0:9598"
              auth:
                strategy: sar
                path: "/metrics"
                verb: "get"
        service:
          ports:
            - name: metrics
              port: 9598
              protocol: TCP
        "#
    );

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            &helm_chart_repo(),
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), &helm_values],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Port forward to Vector's metrics endpoint
    let mut port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{override_name}"),
        9598,
        9598,
    )?;

    port_forward.wait_until_ready().await?;
    let local_addr = port_forward.local_addr_ipv4();

    // Make request without token
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/metrics", local_addr))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 Unauthorized without token"
    );

    drop(vector);

    // Clean up cluster-scoped RBAC resources
    cleanup_cluster_resources(
        framework.kubectl_command(),
        &[
            ("clusterrole", &format!("{}-sar-role", namespace)),
            ("clusterrolebinding", &format!("{}-sar-binding", namespace)),
        ],
    )
    .await;

    Ok(())
}

/// Test SAR authentication rejects token without permission
#[tokio::test]
async fn sar_auth_rejects_unauthorized_token() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create cleanup guard to ensure RBAC resources are deleted even on test failure
    let mut cleanup = TestResourceCleanup::new(framework.kubectl_command().to_string());

    // Create namespace first
    // Namespace creation
    create_namespace(framework.kubectl_command(), &namespace).await?;
    cleanup.track_namespace(namespace.clone());

    // Create ServiceAccount for Vector with SAR permissions

    create_service_account(framework.kubectl_command(), &namespace, "vector-sa").await?;

    create_clusterrole_for_sar(
        framework.kubectl_command(),
        &format!("{}-sar-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-sar-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    )
    .await?;
    cleanup.track("clusterrolebinding", format!("{}-sar-binding", namespace));

    // Create ServiceAccount WITHOUT metrics permission

    create_service_account(framework.kubectl_command(), &namespace, "unprivileged-sa").await?;

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        role: Agent
        serviceAccount:
          create: false
          name: vector-sa
        customConfig:
          sources:
            internal_metrics:
              type: internal_metrics
          sinks:
            prometheus:
              type: prometheus_exporter
              inputs: [internal_metrics]
              address: "0.0.0.0:9598"
              auth:
                strategy: sar
                path: "/metrics"
                verb: "get"
        service:
          ports:
            - name: metrics
              port: 9598
              protocol: TCP
        "#
    );

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            &helm_chart_repo(),
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), &helm_values],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Port forward to Vector's metrics endpoint
    let mut port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{override_name}"),
        9598,
        9598,
    )?;

    port_forward.wait_until_ready().await?;
    let local_addr = port_forward.local_addr_ipv4();

    // Get unprivileged SA token
    let token =
        get_service_account_token(framework.kubectl_command(), &namespace, "unprivileged-sa")
            .await?;

    // Make request with unauthorized token
    let client = reqwest::Client::new();
    let response = client
        .get(format!("http://{}/metrics", local_addr))
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 Unauthorized with unprivileged token"
    );

    drop(vector);

    // Clean up cluster-scoped RBAC resources
    cleanup_cluster_resources(
        framework.kubectl_command(),
        &[
            ("clusterrole", &format!("{}-sar-role", namespace)),
            ("clusterrolebinding", &format!("{}-sar-binding", namespace)),
        ],
    )
    .await;

    Ok(())
}

/// Test that non-/metrics routes return 404 without SAR check (short-circuit)
#[tokio::test]
async fn sar_auth_short_circuits_non_metrics_routes() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create cleanup guard to ensure RBAC resources are deleted even on test failure
    let mut cleanup = TestResourceCleanup::new(framework.kubectl_command().to_string());

    // Create namespace first
    // Namespace creation
    create_namespace(framework.kubectl_command(), &namespace).await?;
    cleanup.track_namespace(namespace.clone());

    // Create ServiceAccount for Vector with SAR permissions

    create_service_account(framework.kubectl_command(), &namespace, "vector-sa").await?;

    create_clusterrole_for_sar(
        framework.kubectl_command(),
        &format!("{}-sar-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-sar-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    )
    .await?;
    cleanup.track("clusterrolebinding", format!("{}-sar-binding", namespace));

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        role: Agent
        serviceAccount:
          create: false
          name: vector-sa
        customConfig:
          sources:
            internal_metrics:
              type: internal_metrics
          sinks:
            prometheus:
              type: prometheus_exporter
              inputs: [internal_metrics]
              address: "0.0.0.0:9598"
              auth:
                strategy: sar
                path: "/metrics"
                verb: "get"
        service:
          ports:
            - name: metrics
              port: 9598
              protocol: TCP
        "#
    );

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            &helm_chart_repo(),
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), &helm_values],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Port forward to Vector's metrics endpoint
    let mut port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{override_name}"),
        9598,
        9598,
    )?;

    port_forward.wait_until_ready().await?;
    let local_addr = port_forward.local_addr_ipv4();

    let client = reqwest::Client::new();

    // Test various non-/metrics paths - should all return 404 immediately
    // without triggering SAR (we test this by not providing any token)
    let invalid_paths = vec!["/", "/health", "/api", "/foo", "/metrics/extra"];

    for path in invalid_paths {
        let response = client
            .get(format!("http://{}{}", local_addr, path))
            .send()
            .await?;

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Path {} should return 404 without auth check",
            path
        );
    }

    drop(vector);

    // Clean up cluster-scoped RBAC resources
    cleanup_cluster_resources(
        framework.kubectl_command(),
        &[
            ("clusterrole", &format!("{}-sar-role", namespace)),
            ("clusterrolebinding", &format!("{}-sar-binding", namespace)),
        ],
    )
    .await;

    Ok(())
}

/// Test allowed_user filter restricts access to specific identity
#[tokio::test]
async fn sar_auth_allowed_user_filter() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create cleanup guard to ensure RBAC resources are deleted even on test failure
    let mut cleanup = TestResourceCleanup::new(framework.kubectl_command().to_string());

    // Create namespace first
    // Namespace creation
    create_namespace(framework.kubectl_command(), &namespace).await?;
    cleanup.track_namespace(namespace.clone());

    // Create ServiceAccount for Vector with SAR permissions

    create_service_account(framework.kubectl_command(), &namespace, "vector-sa").await?;

    create_clusterrole_for_sar(
        framework.kubectl_command(),
        &format!("{}-sar-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-sar-role", namespace));

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    )
    .await?;
    cleanup.track("clusterrolebinding", format!("{}-sar-binding", namespace));

    // Create two ServiceAccounts, both with metrics permission

    create_service_account(framework.kubectl_command(), &namespace, "allowed-sa").await?;

    create_service_account(framework.kubectl_command(), &namespace, "denied-sa").await?;

    create_clusterrole_for_metrics(
        framework.kubectl_command(),
        &format!("{}-metrics-role", namespace),
    )
    .await?;
    cleanup.track("clusterrole", format!("{}-metrics-role", namespace));

    // Both get metrics RBAC permission
    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-allowed-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "allowed-sa",
        &namespace,
    )
    .await?;
    cleanup.track(
        "clusterrolebinding",
        format!("{}-allowed-binding", namespace),
    );

    create_clusterrolebinding(
        framework.kubectl_command(),
        &format!("{}-denied-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "denied-sa",
        &namespace,
    )
    .await?;
    cleanup.track(
        "clusterrolebinding",
        format!("{}-denied-binding", namespace),
    );

    // Deploy Vector with SAR auth and allowed_user filter
    let allowed_user_identity = format!("system:serviceaccount:{}:allowed-sa", namespace);
    let helm_values = formatdoc!(
        r#"
        role: Agent
        serviceAccount:
          create: false
          name: vector-sa
        customConfig:
          sources:
            internal_metrics:
              type: internal_metrics
          sinks:
            prometheus:
              type: prometheus_exporter
              inputs: [internal_metrics]
              address: "0.0.0.0:9598"
              auth:
                strategy: sar
                path: "/metrics"
                verb: "get"
                allowed_user: "{}"
        service:
          ports:
            - name: metrics
              port: 9598
              protocol: TCP
        "#,
        allowed_user_identity
    );

    let vector = framework
        .helm_chart(
            &namespace,
            "vector",
            "vector",
            &helm_chart_repo(),
            VectorConfig {
                custom_helm_values: vec![&config_override_name(&override_name, true), &helm_values],
                ..Default::default()
            },
        )
        .await?;

    framework
        .wait_for_rollout(
            &namespace,
            &format!("daemonset/{override_name}"),
            vec!["--timeout=60s"],
        )
        .await?;

    // Port forward to Vector's metrics endpoint
    let mut port_forward = framework.port_forward(
        &namespace,
        &format!("daemonset/{override_name}"),
        9598,
        9598,
    )?;

    port_forward.wait_until_ready().await?;
    let local_addr = port_forward.local_addr_ipv4();

    let client = reqwest::Client::new();

    // Test with allowed SA - should succeed
    let allowed_token =
        get_service_account_token(framework.kubectl_command(), &namespace, "allowed-sa").await?;
    let response = client
        .get(format!("http://{}/metrics", local_addr))
        .header(header::AUTHORIZATION, format!("Bearer {}", allowed_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Allowed user should be granted access"
    );

    // Test with denied SA - should fail even though it has metrics RBAC
    let denied_token =
        get_service_account_token(framework.kubectl_command(), &namespace, "denied-sa").await?;
    let response = client
        .get(format!("http://{}/metrics", local_addr))
        .header(header::AUTHORIZATION, format!("Bearer {}", denied_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Denied user should be rejected by allowed_user filter"
    );

    drop(vector);

    // Clean up cluster-scoped RBAC resources
    cleanup_cluster_resources(
        framework.kubectl_command(),
        &[
            ("clusterrole", &format!("{}-sar-role", namespace)),
            ("clusterrolebinding", &format!("{}-sar-binding", namespace)),
            ("clusterrole", &format!("{}-metrics-role", namespace)),
            (
                "clusterrolebinding",
                &format!("{}-allowed-binding", namespace),
            ),
            (
                "clusterrolebinding",
                &format!("{}-denied-binding", namespace),
            ),
        ],
    )
    .await;

    Ok(())
}
