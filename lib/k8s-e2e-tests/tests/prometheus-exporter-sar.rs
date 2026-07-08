#![allow(clippy::await_holding_lock)]

use indoc::{formatdoc, indoc};
use k8s_e2e_tests::*;
use k8s_openapi::api::{
    core::v1::ServiceAccount,
    rbac::v1::{ClusterRole, ClusterRoleBinding, PolicyRule, RoleRef, Subject},
};
use k8s_test_framework::{
    lock, namespace, test_pod, vector::Config as VectorConfig, wait_for_resource::WaitFor,
};
use reqwest::{StatusCode, header};

const VECTOR_NAMESPACE: &str = "vector-test";

/// Helper to create a ServiceAccount
fn make_service_account(namespace: &str, name: &str) -> ServiceAccount {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    ServiceAccount {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            namespace: Some(namespace.to_owned()),
            ..ObjectMeta::default()
        },
        ..ServiceAccount::default()
    }
}

/// Helper to create a ClusterRole for SAR
fn make_clusterrole_for_sar(name: &str) -> ClusterRole {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    ClusterRole {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            ..ObjectMeta::default()
        },
        rules: Some(vec![
            PolicyRule {
                api_groups: Some(vec!["authentication.k8s.io".to_string()]),
                resources: Some(vec!["tokenreviews".to_string()]),
                verbs: vec!["create".to_string()],
                ..PolicyRule::default()
            },
            PolicyRule {
                api_groups: Some(vec!["authorization.k8s.io".to_string()]),
                resources: Some(vec!["subjectaccessreviews".to_string()]),
                verbs: vec!["create".to_string()],
                ..PolicyRule::default()
            },
        ]),
        ..ClusterRole::default()
    }
}

/// Helper to create a ClusterRole for metrics access
fn make_clusterrole_for_metrics(name: &str) -> ClusterRole {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    ClusterRole {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            ..ObjectMeta::default()
        },
        rules: Some(vec![PolicyRule {
            non_resource_urls: Some(vec!["/metrics".to_string()]),
            verbs: vec!["get".to_string()],
            ..PolicyRule::default()
        }]),
        ..ClusterRole::default()
    }
}

/// Helper to create a ClusterRoleBinding
fn make_clusterrolebinding(
    name: &str,
    role_name: &str,
    sa_name: &str,
    sa_namespace: &str,
) -> ClusterRoleBinding {
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta;

    ClusterRoleBinding {
        metadata: ObjectMeta {
            name: Some(name.to_owned()),
            ..ObjectMeta::default()
        },
        role_ref: RoleRef {
            api_group: "rbac.authorization.k8s.io".to_string(),
            kind: "ClusterRole".to_string(),
            name: role_name.to_string(),
        },
        subjects: Some(vec![Subject {
            kind: "ServiceAccount".to_string(),
            name: sa_name.to_string(),
            namespace: Some(sa_namespace.to_string()),
            ..Subject::default()
        }]),
    }
}

/// Helper to get ServiceAccount token from K8s
async fn get_service_account_token(
    framework: &k8s_test_framework::Framework,
    namespace: &str,
    sa_name: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    use k8s_openapi::api::core::v1::Secret;
    use k8s_test_framework::Interface;

    // Wait a bit for token to be created
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get secrets for the service account
    let secrets: k8s_openapi::List<Secret> = framework
        .interface
        .client
        .list(namespace, &Default::default())
        .await?;

    // Find the token secret for this SA
    for secret in secrets.items {
        if let Some(annotations) = &secret.metadata.annotations {
            if let Some(sa) = annotations.get("kubernetes.io/service-account.name") {
                if sa == sa_name {
                    if let Some(data) = &secret.data {
                        if let Some(token) = data.get("token") {
                            let token_str = String::from_utf8(token.0.clone())?;
                            return Ok(token_str);
                        }
                    }
                }
            }
        }
    }

    Err("Failed to find ServiceAccount token".into())
}

/// Test basic SAR authentication with valid token
#[tokio::test]
async fn sar_auth_with_valid_token() -> Result<(), Box<dyn std::error::Error>> {
    let _guard = lock();
    init();

    let namespace = get_namespace();
    let framework = make_framework();
    let override_name = get_override_name(&namespace, "vector");

    // Create ServiceAccount for Vector with SAR permissions
    let vector_sa = make_service_account(&namespace, "vector-sa");
    framework.create(vector_sa).await?;

    let sar_role = make_clusterrole_for_sar(&format!("{}-sar-role", namespace));
    framework.create(sar_role).await?;

    let sar_binding = make_clusterrolebinding(
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    );
    framework.create(sar_binding).await?;

    // Create ServiceAccount for Prometheus scraper with metrics permission
    let prom_sa = make_service_account(&namespace, "prometheus-sa");
    framework.create(prom_sa).await?;

    let metrics_role = make_clusterrole_for_metrics(&format!("{}-metrics-role", namespace));
    framework.create(metrics_role).await?;

    let metrics_binding = make_clusterrolebinding(
        &format!("{}-metrics-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "prometheus-sa",
        &namespace,
    );
    framework.create(metrics_binding).await?;

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        serviceAccount:
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

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get Prometheus SA token
    let token = get_service_account_token(&framework, &namespace, "prometheus-sa").await?;

    // Make authenticated request with valid token
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:9598/metrics")
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

    // Create ServiceAccount for Vector with SAR permissions
    let vector_sa = make_service_account(&namespace, "vector-sa");
    framework.create(vector_sa).await?;

    let sar_role = make_clusterrole_for_sar(&format!("{}-sar-role", namespace));
    framework.create(sar_role).await?;

    let sar_binding = make_clusterrolebinding(
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    );
    framework.create(sar_binding).await?;

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        serviceAccount:
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

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Make request without token
    let client = reqwest::Client::new();
    let response = client.get("http://localhost:9598/metrics").send().await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 Unauthorized without token"
    );

    drop(vector);
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

    // Create ServiceAccount for Vector with SAR permissions
    let vector_sa = make_service_account(&namespace, "vector-sa");
    framework.create(vector_sa).await?;

    let sar_role = make_clusterrole_for_sar(&format!("{}-sar-role", namespace));
    framework.create(sar_role).await?;

    let sar_binding = make_clusterrolebinding(
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    );
    framework.create(sar_binding).await?;

    // Create ServiceAccount WITHOUT metrics permission
    let unprivileged_sa = make_service_account(&namespace, "unprivileged-sa");
    framework.create(unprivileged_sa).await?;

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        serviceAccount:
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

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Get unprivileged SA token
    let token = get_service_account_token(&framework, &namespace, "unprivileged-sa").await?;

    // Make request with unauthorized token
    let client = reqwest::Client::new();
    let response = client
        .get("http://localhost:9598/metrics")
        .header(header::AUTHORIZATION, format!("Bearer {}", token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected 401 Unauthorized with unprivileged token"
    );

    drop(vector);
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

    // Create ServiceAccount for Vector with SAR permissions
    let vector_sa = make_service_account(&namespace, "vector-sa");
    framework.create(vector_sa).await?;

    let sar_role = make_clusterrole_for_sar(&format!("{}-sar-role", namespace));
    framework.create(sar_role).await?;

    let sar_binding = make_clusterrolebinding(
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    );
    framework.create(sar_binding).await?;

    // Deploy Vector with SAR auth
    let helm_values = formatdoc!(
        r#"
        serviceAccount:
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

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = reqwest::Client::new();

    // Test various non-/metrics paths - should all return 404 immediately
    // without triggering SAR (we test this by not providing any token)
    let invalid_paths = vec!["/", "/health", "/api", "/foo", "/metrics/extra"];

    for path in invalid_paths {
        let response = client
            .get(format!("http://localhost:9598{}", path))
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

    // Create ServiceAccount for Vector with SAR permissions
    let vector_sa = make_service_account(&namespace, "vector-sa");
    framework.create(vector_sa).await?;

    let sar_role = make_clusterrole_for_sar(&format!("{}-sar-role", namespace));
    framework.create(sar_role).await?;

    let sar_binding = make_clusterrolebinding(
        &format!("{}-sar-binding", namespace),
        &format!("{}-sar-role", namespace),
        "vector-sa",
        &namespace,
    );
    framework.create(sar_binding).await?;

    // Create two ServiceAccounts, both with metrics permission
    let allowed_sa = make_service_account(&namespace, "allowed-sa");
    framework.create(allowed_sa).await?;

    let denied_sa = make_service_account(&namespace, "denied-sa");
    framework.create(denied_sa).await?;

    let metrics_role = make_clusterrole_for_metrics(&format!("{}-metrics-role", namespace));
    framework.create(metrics_role).await?;

    // Both get metrics RBAC permission
    let allowed_binding = make_clusterrolebinding(
        &format!("{}-allowed-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "allowed-sa",
        &namespace,
    );
    framework.create(allowed_binding).await?;

    let denied_binding = make_clusterrolebinding(
        &format!("{}-denied-binding", namespace),
        &format!("{}-metrics-role", namespace),
        "denied-sa",
        &namespace,
    );
    framework.create(denied_binding).await?;

    // Deploy Vector with SAR auth and allowed_user filter
    let allowed_user_identity = format!("system:serviceaccount:{}:allowed-sa", namespace);
    let helm_values = formatdoc!(
        r#"
        serviceAccount:
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

    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    let client = reqwest::Client::new();

    // Test with allowed SA - should succeed
    let allowed_token = get_service_account_token(&framework, &namespace, "allowed-sa").await?;
    let response = client
        .get("http://localhost:9598/metrics")
        .header(header::AUTHORIZATION, format!("Bearer {}", allowed_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Allowed user should be granted access"
    );

    // Test with denied SA - should fail even though it has metrics RBAC
    let denied_token = get_service_account_token(&framework, &namespace, "denied-sa").await?;
    let response = client
        .get("http://localhost:9598/metrics")
        .header(header::AUTHORIZATION, format!("Bearer {}", denied_token))
        .send()
        .await?;

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Denied user should be rejected by allowed_user filter"
    );

    drop(vector);
    Ok(())
}
