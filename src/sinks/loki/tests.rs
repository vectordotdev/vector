use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use futures::{StreamExt, stream};
use hyper::Body;
use vector_lib::config::{log_schema, proxy::ProxyConfig};
use vrl::event_path;

use super::{config::LokiConfig, healthcheck::healthcheck, sink::LokiSink};
use crate::{
    http::HttpClient,
    sinks::{
        prelude::*,
        util::{
            HttpEndpoint, UriSerde,
            test::{build_test_server, build_test_server_generic, load_sink},
        },
    },
    test_util,
};

#[test]
fn generate_config() {
    test_util::test_generate_config::<LokiConfig>();
}

#[tokio::test]
async fn interpolate_labels() {
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
        endpoint = "http://localhost:3100"
        labels = {label1 = "l1-{{ foo }}", label2 = "some-static-label", label3 = "l3-{{ foo }}", "k-{{ foo }}" = "v-{{ foo }}"}
        encoding.codec = "json"
        remove_label_fields = true
    "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = Event::Log(LogEvent::from("hello world"));

    e1.as_mut_log().insert(event_path!("foo"), "bar");

    let mut record = sink.encoder.encode_event(e1).unwrap();

    // HashMap -> Vec doesn't like keeping ordering
    record.labels.sort();

    // The final event should have timestamps and labels removed
    let expected_line = serde_json::to_string(&serde_json::json!({
        "message": "hello world",
    }))
    .unwrap();

    assert_eq!(record.event.event, expected_line);

    assert_eq!(record.labels[0], ("k-bar".to_string(), "v-bar".to_string()));
    assert_eq!(
        record.labels[1],
        ("label1".to_string(), "l1-bar".to_string())
    );
    assert_eq!(
        record.labels[2],
        ("label2".to_string(), "some-static-label".to_string())
    );
    // make sure we can reuse fields across labels.
    assert_eq!(
        record.labels[3],
        ("label3".to_string(), "l3-bar".to_string())
    );
}

#[tokio::test]
async fn use_label_from_dropped_fields() {
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://localhost:3100"
            labels.bar = "bar-{{ foo }}"
            encoding.codec = "json"
            encoding.except_fields = ["foo"]
        "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = Event::Log(LogEvent::from("hello world"));

    e1.as_mut_log().insert(event_path!("foo"), "bar");

    let record = sink.encoder.encode_event(e1).unwrap();

    let expected_line = serde_json::to_string(&serde_json::json!({
        "message": "hello world",
    }))
    .unwrap();

    assert_eq!(record.event.event, expected_line);

    assert_eq!(record.labels[0], ("bar".to_string(), "bar-bar".to_string()));
}

#[tokio::test]
async fn healthcheck_includes_auth() {
    let (mut config, cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding.codec = "json"
			auth.strategy = "basic"
			auth.user = "username"
			auth.password = "some_password"
        "#,
    )
    .unwrap();

    let (_guard, addr) = test_util::addr::next_addr();
    let endpoint = format!("http://{addr}");
    config.endpoint = HttpEndpoint::parse(&endpoint).unwrap();

    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let tls =
        TlsSettings::from_options(config.tls.as_ref()).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(
        config.endpoint.clone(),
        config.auth.clone(),
        cx.healthcheck.uri,
        client,
    )
    .await
    .expect("healthcheck failed");

    let output = rx.take(1).collect::<Vec<_>>().await;
    assert_eq!(
        Some(&http::header::HeaderValue::from_static(
            "Basic dXNlcm5hbWU6c29tZV9wYXNzd29yZA=="
        )),
        output[0].0.headers.get("authorization")
    );
}

#[tokio::test]
async fn healthcheck_uses_configured_uri_with_uri_auth_precedence() {
    let (config, _cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding.codec = "json"
            auth.strategy = "basic"
            auth.user = "username"
            auth.password = "some_password"
        "#,
    )
    .unwrap();

    let (_guard, addr) = test_util::addr::next_addr();
    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    // Credentials embedded in the configured healthcheck URI take precedence
    // over the sink auth ("user:pass", not "username:some_password"). The path
    // `/health` shares no segment with the default `/ready` append path, so the
    // assertions below prove the configured URI is used verbatim.
    let healthcheck_uri: UriSerde = format!("http://user:pass@{addr}/health")
        .parse()
        .expect("could not create healthcheck URI");

    let tls =
        TlsSettings::from_options(config.tls.as_ref()).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(
        config.endpoint.clone(),
        config.auth.clone(),
        Some(healthcheck_uri),
        client,
    )
    .await
    .expect("healthcheck failed");

    let (parts, _) = rx
        .take(1)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .next()
        .expect("healthcheck made no request");
    // The configured URI is probed verbatim...
    assert_eq!(parts.uri.path(), "/health");
    // ...and the default `ready` append path is not used.
    assert_ne!(parts.uri.path(), "/ready");
    assert_eq!(
        parts.headers.get("authorization"),
        Some(&http::header::HeaderValue::from_static(
            "Basic dXNlcjpwYXNz"
        )),
    );
}

#[tokio::test]
async fn sink_uses_validated_endpoint_and_auth() {
    let (_guard, addr) = test_util::addr::next_addr();
    let config = format!(
        r#"
            endpoint = "http://user:pass@{addr}"
            path = "/custom/loki/push"
            labels = {{test_name = "placeholder"}}
            encoding.codec = "json"
        "#
    );
    let (config, cx) = load_sink::<LokiConfig>(&config).unwrap();
    let client = config.build_client(cx).unwrap();
    let sink = LokiSink::new(config, client).unwrap();

    let (mut rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    Box::new(sink)
        .run(stream::iter([Event::Log(LogEvent::from("hello world"))]).boxed())
        .await
        .unwrap();

    let (parts, _) = rx.next().await.expect("server should receive a request");
    assert_eq!(parts.uri.path(), "/custom/loki/push");
    assert_eq!(
        parts.headers.get(http::header::AUTHORIZATION).unwrap(),
        "Basic dXNlcjpwYXNz"
    );
}

#[tokio::test]
async fn healthcheck_fallback_requests_base_path_with_trailing_slash() {
    let (mut config, _cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://localhost:3100"
            labels = {test_name = "placeholder"}
            encoding.codec = "json"
        "#,
    )
    .unwrap();

    let (_guard, addr) = test_util::addr::next_addr();
    let endpoint = format!("http://{addr}/loki///");
    config.endpoint = HttpEndpoint::parse(&endpoint).unwrap();

    // The `/ready` request returns 404 to trigger the fallback. The generic
    // server records the successful fallback request and shuts down with its
    // returned trigger.
    let first_request = Arc::new(AtomicBool::new(true));
    let responder_state = Arc::clone(&first_request);
    let (mut rx, _trigger, server) = build_test_server_generic(addr, move || {
        let status = if responder_state.swap(false, Ordering::Relaxed) {
            http::StatusCode::NOT_FOUND
        } else {
            http::StatusCode::OK
        };
        http::Response::builder()
            .status(status)
            .body(Body::empty())
            .unwrap()
    });
    tokio::spawn(server);

    let tls =
        TlsSettings::from_options(config.tls.as_ref()).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(config.endpoint.clone(), config.auth.clone(), None, client)
        .await
        .expect("healthcheck failed");

    // The successful fallback must probe the base path with a trailing slash
    // (`/loki/`, not `/loki`), matching the pre-`HttpEndpoint` behavior.
    let (parts, _) = rx.next().await.expect("fallback request should succeed");
    assert_eq!(parts.uri.path(), "/loki/");
}

#[tokio::test]
async fn healthcheck_grafana_cloud() {
    test_util::trace_init();
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://logs-prod-us-central1.grafana.net"
            encoding.codec = "json"
            labels = {test_name = "placeholder"}
        "#,
    )
    .unwrap();

    let tls =
        TlsSettings::from_options(config.tls.as_ref()).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(
        config.endpoint.clone(),
        config.auth.clone(),
        cx.healthcheck.uri,
        client,
    )
    .await
    .expect("healthcheck failed");
}

#[tokio::test]
async fn timestamp_out_of_range() {
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
        endpoint = "http://localhost:3100"
        labels = {label1 = "l1-{{ foo }}", label2 = "some-static-label", label3 = "l3-{{ foo }}", "k-{{ foo }}" = "v-{{ foo }}"}
        encoding.codec = "json"
    "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = LogEvent::from("hello world");
    if let Some(timestamp_key) = log_schema().timestamp_key_target_path() {
        let date = chrono::NaiveDate::from_ymd_opt(1677, 9, 21)
            .unwrap()
            .and_hms_nano_opt(0, 12, 43, 145_224_191)
            .unwrap()
            .and_local_timezone(chrono::Utc)
            .unwrap();
        e1.insert(timestamp_key, date);
    }
    let e1 = Event::Log(e1);

    assert!(sink.encoder.encode_event(e1).is_none());
}

#[tokio::test]
async fn structured_metadata_as_json() {
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
        endpoint = "http://localhost:3100"
        labels = {test = "structured_metadata"}
        structured_metadata.bar = "bar-{{ foo }}"
        encoding.codec = "json"
        encoding.except_fields = ["foo"]
        "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = Event::Log(LogEvent::from("hello world"));
    e1.as_mut_log().insert(event_path!("foo"), "bar");

    let event = sink.encoder.encode_event(e1).unwrap();
    let body = serde_json::json!(event.event);
    let expected_metadata = serde_json::json!({"bar": "bar-bar"});

    assert_eq!(body[2], expected_metadata);
}
