use vector_lib::config::proxy::ProxyConfig;

use super::{config::LokiConfig, healthcheck::healthcheck, sink::LokiSink};
use crate::{
    http::HttpClient,
    sinks::prelude::*,
    sinks::util::test::{build_test_server, load_sink},
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
        labels = {label1 = "{{ foo }}", label2 = "some-static-label", label3 = "{{ foo }}", "{{ foo }}" = "{{ foo }}"}
        encoding.codec = "json"
        remove_label_fields = true
    "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = Event::Log(LogEvent::from("hello world"));

    e1.as_mut_log().insert("foo", "bar");

    let mut record = sink.encoder.encode_event(e1).unwrap();

    // HashMap -> Vec doesn't like keeping ordering
    record.labels.sort();

    // The final event should have timestamps and labels removed
    let expected_line = serde_json::to_string(&serde_json::json!({
        "message": "hello world",
    }))
    .unwrap();

    assert_eq!(record.event.event, expected_line);

    assert_eq!(record.labels[0], ("bar".to_string(), "bar".to_string()));
    assert_eq!(record.labels[1], ("label1".to_string(), "bar".to_string()));
    assert_eq!(
        record.labels[2],
        ("label2".to_string(), "some-static-label".to_string())
    );
    // make sure we can reuse fields across labels.
    assert_eq!(record.labels[3], ("label3".to_string(), "bar".to_string()));
}

#[tokio::test]
async fn use_label_from_dropped_fields() {
    let (config, cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://localhost:3100"
            labels.bar = "{{ foo }}"
            encoding.codec = "json"
            encoding.except_fields = ["foo"]
        "#,
    )
    .unwrap();
    let client = config.build_client(cx).unwrap();
    let mut sink = LokiSink::new(config, client).unwrap();

    let mut e1 = Event::Log(LogEvent::from("hello world"));

    e1.as_mut_log().insert("foo", "bar");

    let record = sink.encoder.encode_event(e1).unwrap();

    let expected_line = serde_json::to_string(&serde_json::json!({
        "message": "hello world",
    }))
    .unwrap();

    assert_eq!(record.event.event, expected_line);

    assert_eq!(record.labels[0], ("bar".to_string(), "bar".to_string()));
}

#[tokio::test]
async fn healthcheck_includes_auth() {
    let (mut config, _cx) = load_sink::<LokiConfig>(
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

    let addr = test_util::next_addr();
    let endpoint = format!("http://{}", addr);
    config.endpoint = endpoint
        .clone()
        .parse::<http::Uri>()
        .expect("could not create URI")
        .into();

    let (rx, _trigger, server) = build_test_server(addr);
    tokio::spawn(server);

    let tls = TlsSettings::from_options(&config.tls).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(config.clone(), client)
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
async fn healthcheck_grafana_cloud() {
    test_util::trace_init();
    let (config, _cx) = load_sink::<LokiConfig>(
        r#"
            endpoint = "http://logs-prod-us-central1.grafana.net"
            encoding.codec = "json"
            labels = {test_name = "placeholder"}
        "#,
    )
    .unwrap();

    let tls = TlsSettings::from_options(&config.tls).expect("could not create TLS settings");
    let proxy = ProxyConfig::default();
    let client = HttpClient::new(tls, &proxy).expect("could not create HTTP client");

    healthcheck(config, client)
        .await
        .expect("healthcheck failed");
}
