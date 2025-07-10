use std::time::Duration;

use futures::{future::ready, stream};
use http::Response;
use hyper::body;
use openssl::{base64, hash, pkey, sign};
use tokio::time::timeout;
use vector_lib::config::log_schema;

use super::{
    config::{default_host, AzureMonitorLogsConfig},
    sink::JsonEncoding,
};
use crate::{
    event::LogEvent,
    sinks::{prelude::*, util::encoding::Encoder},
    test_util::{
        components::{run_and_assert_sink_compliance, SINK_TAGS},
        http::{always_200_response, spawn_blackhole_http_server},
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<AzureMonitorLogsConfig>();
}

#[tokio::test]
async fn component_spec_compliance() {
    let mock_endpoint = spawn_blackhole_http_server(always_200_response).await;

    let config = AzureMonitorLogsConfig {
        shared_key: "ZnNkO2Zhc2RrbGZqYXNkaixmaG5tZXF3dWlsamtmYXNjZmouYXNkbmZrbHFhc2ZtYXNrbA=="
            .to_string()
            .into(),
        ..Default::default()
    };

    let context = SinkContext::default();
    let (sink, _healthcheck) = config
        .build_inner(context, mock_endpoint.into())
        .await
        .unwrap();

    let event = Event::Log(LogEvent::from("simple message"));
    run_and_assert_sink_compliance(sink, stream::once(ready(event)), &SINK_TAGS).await;
}

#[tokio::test]
async fn fails_missing_creds() {
    let config: AzureMonitorLogsConfig = toml::from_str(
        r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = ""
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
    )
    .unwrap();
    if config.build(SinkContext::default()).await.is_ok() {
        panic!("config.build failed to error");
    }
}

#[test]
fn correct_host() {
    let config_default = toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
        "#,
        )
        .expect("Config parsing failed without custom host");
    assert_eq!(config_default.host, default_host());

    let config_cn = toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
            host = "ods.opinsights.azure.cn"
        "#,
        )
        .expect("Config parsing failed with .cn custom host");
    assert_eq!(config_cn.host, "ods.opinsights.azure.cn");
}

#[tokio::test]
async fn fails_invalid_base64() {
    let config: AzureMonitorLogsConfig = toml::from_str(
        r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "1Qs77Vz40+iDMBBTRmROKJwnEX"
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
    )
    .unwrap();
    if config.build(SinkContext::default()).await.is_ok() {
        panic!("config.build failed to error");
    }
}

#[test]
fn fails_config_missing_fields() {
    toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .expect_err("Config parsing failed to error with missing log_type");

    toml::from_str::<AzureMonitorLogsConfig>(
        r#"
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
    )
    .expect_err("Config parsing failed to error with missing shared_key");

    toml::from_str::<AzureMonitorLogsConfig>(
            r#"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
        "#,
        )
        .expect_err("Config parsing failed to error with missing customer_id");
}

fn insert_timestamp_kv(log: &mut LogEvent) -> (String, String) {
    let now = chrono::Utc::now();

    let timestamp_value = now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    log.insert(log_schema().timestamp_key_target_path().unwrap(), now);

    (
        log_schema().timestamp_key().unwrap().to_string(),
        timestamp_value,
    )
}

fn build_authorization_header_value(
    shared_key: &pkey::PKey<pkey::Private>,
    customer_id: &str,
    rfc1123date: &str,
    len: usize,
) -> crate::Result<String> {
    let string_to_hash =
        format!("POST\n{len}\napplication/json\nx-ms-date:{rfc1123date}\n/api/logs");
    let mut signer = sign::Signer::new(hash::MessageDigest::sha256(), shared_key)?;
    signer.update(string_to_hash.as_bytes())?;

    let signature = signer.sign_to_vec()?;
    let signature_base64 = base64::encode_block(&signature);

    Ok(format!("SharedKey {customer_id}:{signature_base64}"))
}

#[tokio::test]
async fn correct_request() {
    let config: AzureMonitorLogsConfig = toml::from_str(
            r#"
            # random GUID and random 64 Base-64 encoded bytes
            customer_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
            shared_key = "SERsIYhgMVlJB6uPsq49gCxNiruf6v0vhMYE+lfzbSGcXjdViZdV/e5pEMTYtw9f8SkVLf4LFlLCc2KxtRZfCA=="
            log_type = "Vector"
            azure_resource_id = "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
        "#,
        )
        .unwrap();

    let mut log1 = [("message", "hello")].iter().copied().collect::<LogEvent>();
    let (timestamp_key1, timestamp_value1) = insert_timestamp_kv(&mut log1);

    let mut log2 = [("message", "world")].iter().copied().collect::<LogEvent>();
    let (timestamp_key2, timestamp_value2) = insert_timestamp_kv(&mut log2);

    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let mock_endpoint = spawn_blackhole_http_server(move |request| {
        let tx = tx.clone();
        async move {
            tx.send(request).await.unwrap();
            Ok(Response::new(hyper::Body::empty()))
        }
    })
    .await;

    let context = SinkContext::default();
    let (sink, _healthcheck) = config
        .build_inner(context, mock_endpoint.into())
        .await
        .unwrap();

    run_and_assert_sink_compliance(sink, stream::iter(vec![log1, log2]), &SINK_TAGS).await;

    let request = timeout(Duration::from_millis(500), rx.recv())
        .await
        .unwrap()
        .unwrap();

    let (parts, body) = request.into_parts();
    assert_eq!(&parts.method.to_string(), "POST");

    let body = body::to_bytes(body).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body[..]).unwrap();
    let expected_json = serde_json::json!([
        {
            timestamp_key1: timestamp_value1,
            "message": "hello"
        },
        {
            timestamp_key2: timestamp_value2,
            "message": "world"
        }
    ]);
    assert_eq!(json, expected_json);

    let headers = parts.headers;

    let rfc1123date = headers.get("x-ms-date").unwrap();
    let shared_key = config.build_shared_key().unwrap();
    let auth_expected = build_authorization_header_value(
        &shared_key,
        &config.customer_id,
        rfc1123date.to_str().unwrap(),
        body.len(),
    )
    .unwrap();
    let authorization = headers.get("authorization").unwrap();
    assert_eq!(authorization.to_str().unwrap(), &auth_expected);

    let log_type = headers.get("log-type").unwrap();
    assert_eq!(log_type.to_str().unwrap(), "Vector");

    let time_generated_field = headers.get("time-generated-field").unwrap();
    let timestamp_key = log_schema().timestamp_key();
    assert_eq!(
        time_generated_field.to_str().unwrap(),
        timestamp_key.unwrap().to_string().as_str()
    );

    let azure_resource_id = headers.get("x-ms-azureresourceid").unwrap();
    assert_eq!(
        azure_resource_id.to_str().unwrap(),
        "97ce69d9-b4be-4241-8dbd-d265edcf06c4"
    );

    assert_eq!(
        &parts.uri.path_and_query().unwrap().to_string(),
        "/api/logs?api-version=2016-04-01"
    );
}

#[test]
fn encode_valid() {
    let mut log = [("message", "hello world")]
        .iter()
        .copied()
        .collect::<LogEvent>();
    let (timestamp_key, timestamp_value) = insert_timestamp_kv(&mut log);

    let event = Event::from(log);
    let encoder = JsonEncoding::new(Default::default(), log_schema().timestamp_key().cloned());
    let mut encoded = vec![];
    encoder.encode_input(vec![event], &mut encoded).unwrap();
    let expected_json = serde_json::json!([{
        timestamp_key: timestamp_value,
        "message": "hello world"
    }]);
    let json: serde_json::Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(json, expected_json);
}
