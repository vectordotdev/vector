use std::collections::HashMap;

use http::Uri;
use tokio::time::Duration;
use vector_lib::{
    codecs::{
        CharacterDelimitedDecoderConfig,
        decoding::{CharacterDelimitedDecoderOptions, DeserializerConfig, FramingConfig},
    },
    config::LogNamespace,
    event::Event,
};
use warp::{Filter, http::HeaderMap};

use super::HttpClientConfig;
use crate::{
    components::validation::prelude::*,
    http::{ParamType, ParameterValue, QueryParameterValue},
    serde::{default_decoding, default_framing_message_based},
    sources::util::http::HttpMethod,
    test_util::{
        addr::next_addr,
        components::{HTTP_PULL_SOURCE_TAGS, run_and_assert_source_compliance},
        test_generate_config, wait_for_tcp,
    },
};

pub(crate) const INTERVAL: Duration = Duration::from_secs(1);

pub(crate) const TIMEOUT: Duration = Duration::from_secs(1);

/// The happy path should yield at least one event and must emit the required internal events for sources.
pub(crate) async fn run_compliance(config: HttpClientConfig) -> Vec<Event> {
    let events =
        run_and_assert_source_compliance(config, Duration::from_secs(3), &HTTP_PULL_SOURCE_TAGS)
            .await;

    assert!(!events.is_empty());

    events
}

#[test]
fn http_client_generate_config() {
    test_generate_config::<HttpClientConfig>();
}

impl ValidatableComponent for HttpClientConfig {
    fn validation_configuration() -> ValidationConfiguration {
        let uri = Uri::from_static("http://127.0.0.1:9898");

        let config = Self {
            endpoint: uri.to_string(),
            interval: Duration::from_secs(1),
            timeout: Duration::from_secs(1),
            decoding: DeserializerConfig::Json(Default::default()),
            ..Default::default()
        };
        let log_namespace: LogNamespace = config.log_namespace.unwrap_or_default().into();

        let external_resource = ExternalResource::new(
            ResourceDirection::Pull,
            HttpResourceConfig::from_parts(uri, Some(config.method.into())),
            config.get_decoding_config(None),
        );

        ValidationConfiguration::from_source(
            Self::NAME,
            log_namespace,
            vec![ComponentTestCaseConfig::from_source(
                config,
                None,
                Some(external_resource),
            )],
        )
    }
}

register_validatable_component!(HttpClientConfig);

/// Bytes should be decoded and HTTP header set to text/plain.
#[tokio::test]
async fn bytes_decoding() {
    let (_guard, in_addr) = next_addr();

    // validates the Accept header is set correctly for the Bytes codec
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::header::exact("Accept", "text/plain"))
        .map(|| r"A plain text event");

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: default_decoding(),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// JSON with newline delimiter should be decoded and HTTP header set to application/x-ndjson.
#[tokio::test]
async fn json_decoding_newline_delimited() {
    let (_guard, in_addr) = next_addr();

    // validates the Content-Type is set correctly for the Json codec
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::header::exact("Accept", "application/x-ndjson"))
        .map(|| r#"{"data" : "foo"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: FramingConfig::NewlineDelimited(Default::default()),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// JSON with character delimiter should be decoded and HTTP header set to application/json.
#[tokio::test]
async fn json_decoding_character_delimited() {
    let (_guard, in_addr) = next_addr();

    // validates the Content-Type is set correctly for the Json codec
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::header::exact("Accept", "application/json"))
        .map(|| r#"{"data" : "foo"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: FramingConfig::CharacterDelimited(CharacterDelimitedDecoderConfig {
            character_delimited: CharacterDelimitedDecoderOptions {
                delimiter: b',',
                max_length: Some(usize::MAX),
            },
        }),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// HTTP request queries configured by the user should be applied correctly.
#[tokio::test]
async fn request_query_applied() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::query::raw())
        .map(|query| format!(r#"{{"data" : "{query}"}}"#));

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let events = run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint?key1=val1"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::from([
            (
                "key1".to_string(),
                QueryParameterValue::MultiParams(vec![ParameterValue::String("val2".to_string())]),
            ),
            (
                "key2".to_string(),
                QueryParameterValue::MultiParams(vec![
                    ParameterValue::String("val1".to_string()),
                    ParameterValue::String("val2".to_string()),
                ]),
            ),
        ]),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;

    let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

    let expected = HashMap::from([
        (
            "key1".to_string(),
            vec!["val1".to_string(), "val2".to_string()],
        ),
        (
            "key2".to_string(),
            vec!["val1".to_string(), "val2".to_string()],
        ),
    ]);

    for log in logs {
        let query = log.get("data").expect("data must be available");
        let mut got: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in
            url::form_urlencoded::parse(query.as_bytes().expect("byte conversion should succeed"))
        {
            got.entry(k.to_string()).or_default().push(v.to_string());
        }
        for v in got.values_mut() {
            v.sort();
        }
        assert_eq!(got, expected);
    }
}

/// VRL query parameters should be parsed correctly
#[tokio::test]
async fn request_query_vrl_applied() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::query::raw())
        .map(|query| format!(r#"{{"data" : "{query}"}}"#));

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let events = run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::from([
            // Test a single VRL parameter with concatenation
            (
                "key1".to_string(),
                QueryParameterValue::SingleParam(ParameterValue::Typed {
                    value: "upcase(\"bar\") + \"-\" + md5(\"baz\")".to_string(),
                    r#type: ParamType::Vrl,
                }),
            ),
            // Test multiple parameters with a mixture of string and VRL types
            (
                "key2".to_string(),
                QueryParameterValue::MultiParams(vec![
                    // Check that nested quotes are not stripped
                    ParameterValue::String("\"bob ross\"".to_string()),
                    ParameterValue::Typed {
                        value: "mod(5, 2)".to_string(),
                        r#type: ParamType::Vrl,
                    },
                    ParameterValue::Typed {
                        value: "camelcase(\"input-string\")".to_string(),
                        r#type: ParamType::Vrl,
                    },
                ]),
            ),
            // Test if VRL timestamps are correctly formatted as a raw ISO 8601 string
            (
                "key3".to_string(),
                QueryParameterValue::SingleParam(ParameterValue::Typed {
                    value: "parse_timestamp!(\"10-Oct-2020 16:00+00:00\", format: \"%v %R %:z\")"
                        .to_string(),
                    r#type: ParamType::Vrl,
                }),
            ),
            // Test if other types are formatted correctly
            (
                "key4".to_string(),
                QueryParameterValue::MultiParams(vec![
                    ParameterValue::Typed {
                        value: "to_int!(\"123\")".to_string(),
                        r#type: ParamType::Vrl,
                    },
                    ParameterValue::Typed {
                        value: "to_bool!(\"yes\")".to_string(),
                        r#type: ParamType::Vrl,
                    },
                    ParameterValue::Typed {
                        value: "to_float!(\"-99.9\")".to_string(),
                        r#type: ParamType::Vrl,
                    },
                    ParameterValue::Typed {
                        value: "to_string!(false)".to_string(),
                        r#type: ParamType::Vrl,
                    },
                ]),
            ),
        ]),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;

    let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

    let mut expected = HashMap::from([
        (
            "key1".to_string(),
            vec!["BAR-73feffa4b7f6bb68e44cf984c85f6e88".to_string()],
        ),
        (
            "key2".to_string(),
            vec![
                "\"bob ross\"".to_string(),
                "1".to_string(),
                "inputString".to_string(),
            ],
        ),
        ("key3".to_string(), vec!["2020-10-10T16:00:00Z".to_string()]),
        (
            "key4".to_string(),
            vec![
                "123".to_string(),
                "true".to_string(),
                "-99.9".to_string(),
                "false".to_string(),
            ],
        ),
    ]);

    for v in expected.values_mut() {
        v.sort();
    }

    for log in logs {
        let query = log.get("data").expect("data must be available");
        let mut got: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in
            url::form_urlencoded::parse(query.as_bytes().expect("byte conversion should succeed"))
        {
            got.entry(k.to_string()).or_default().push(v.to_string());
        }
        for v in got.values_mut() {
            v.sort();
        }
        assert_eq!(got, expected);
    }
}

/// VRL query parameters should dynamically update on each request
#[tokio::test]
async fn request_query_vrl_dynamic_updates() {
    let (_guard, in_addr) = next_addr();

    // A handler that returns the query parameters as part of the response
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::query::raw())
        .map(|query| format!(r#"{{"data" : "{query}"}}"#));

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    // The timestamp should be different for each event
    let events = run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: Duration::from_millis(100),
        timeout: TIMEOUT,
        query: HashMap::from([(
            "timestamp".to_string(),
            QueryParameterValue::SingleParam(ParameterValue::Typed {
                value: "to_unix_timestamp(now(), unit: \"milliseconds\")".to_string(),
                r#type: ParamType::Vrl,
            }),
        )]),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;

    let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

    // Make sure we have at least 2 events to check for unique timestamps
    assert!(
        logs.len() >= 2,
        "Expected at least 2 events, got {}",
        logs.len()
    );

    let mut timestamps = Vec::new();
    for log in logs {
        let query = log.get("data").expect("data must be available");
        let query_bytes = query.as_bytes().expect("byte conversion should succeed");

        // Parse the timestamp value
        for (k, v) in url::form_urlencoded::parse(query_bytes) {
            if k == "timestamp" {
                timestamps.push(v.to_string());
            }
        }
    }

    // Check that timestamps are unique (should be different for each request)
    let unique_timestamps: std::collections::HashSet<String> = timestamps.iter().cloned().collect();
    assert_eq!(
        timestamps.len(),
        unique_timestamps.len(),
        "Expected all timestamps to be unique"
    );
}

/// HTTP request headers configured by the user should be applied correctly.
#[tokio::test]
async fn headers_applied() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::header::exact("Accept", "text/plain"))
        .and(warp::header::headers_cloned().map(|headers: HeaderMap| {
            let view = headers.get_all("f00");
            let mut iter = view.iter();
            assert_eq!(&"bazz", iter.next().unwrap());
            assert_eq!(&"bizz", iter.next().unwrap());
        }))
        .map(|_| r#"{"data" : "foo"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: default_decoding(),
        framing: default_framing_message_based(),
        headers: HashMap::from([(
            "f00".to_string(),
            vec!["bazz".to_string(), "bizz".to_string()],
        )]),
        method: HttpMethod::Get,
        body: None,
        auth: None,
        tls: None,
        log_namespace: None,
    })
    .await;
}

/// ACCEPT HTTP request headers configured by the user should take precedence
#[tokio::test]
async fn accept_header_override() {
    let (_guard, in_addr) = next_addr();

    // (The Bytes decoder will default to text/plain encoding)
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::header::exact("Accept", "application/json"))
        .map(|| r#"{"data" : "foo"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Bytes,
        framing: default_framing_message_based(),
        headers: HashMap::from([("ACCEPT".to_string(), vec!["application/json".to_string()])]),
        method: HttpMethod::Get,
        body: None,
        auth: None,
        tls: None,
        log_namespace: None,
    })
    .await;
}

/// POST request with JSON body data should send the body correctly
#[tokio::test]
async fn post_with_body() {
    let (_guard, in_addr) = next_addr();

    // Endpoint that echoes back the request body
    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::post())
        .and(warp::header::exact("Content-Type", "application/json"))
        .and(warp::body::bytes())
        .map(|body: bytes::Bytes| {
            // Echo the body back as a string
            String::from_utf8_lossy(&body).to_string()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let test_json = r#"{"key":"value","number":42}"#;

    let events = run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Post,
        body: Some(ParameterValue::String(test_json.to_string())),
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;

    let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

    // Verify the body was echoed back correctly
    for log in logs {
        assert_eq!(log.get("key").unwrap().as_str().unwrap(), "value");
        let number = log.get("number").unwrap();
        match number {
            vector_lib::event::Value::Integer(n) => assert_eq!(*n, 42),
            _ => panic!("Expected integer value"),
        }
    }
}

/// POST request without body should work as before
#[tokio::test]
async fn post_without_body() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::post())
        .map(|| r#"{"data": "success"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Post,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// Custom Content-Type header should override the default
#[tokio::test]
async fn post_with_custom_content_type() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::post())
        .and(warp::header::exact("Content-Type", "text/plain"))
        .map(|| r#"{"data": "success"}"#);

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::from([("Content-Type".to_string(), vec!["text/plain".to_string()])]),
        method: HttpMethod::Post,
        body: Some(ParameterValue::String("plain text body".to_string())),
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// POST request with VRL body should resolve correctly
#[tokio::test]
async fn post_with_vrl_body() {
    let (_guard, in_addr) = next_addr();

    let dummy_endpoint = warp::path!("endpoint")
        .and(warp::post())
        .and(warp::header::exact("Content-Type", "application/json"))
        .and(warp::body::bytes())
        .map(|body: bytes::Bytes| {
            // Echo back the body as a string
            String::from_utf8_lossy(&body).to_string()
        });

    tokio::spawn(warp::serve(dummy_endpoint).run(in_addr));
    wait_for_tcp(in_addr).await;

    let events = run_compliance(HttpClientConfig {
        endpoint: format!("http://{in_addr}/endpoint"),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Post,
        body: Some(ParameterValue::Typed {
            value: r#"encode_json({"message": upcase("hello"), "value": 42})"#.to_string(),
            r#type: ParamType::Vrl,
        }),
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;

    let logs: Vec<_> = events.into_iter().map(|event| event.into_log()).collect();

    // Verify VRL was evaluated correctly
    for log in logs {
        assert_eq!(log.get("message").unwrap().as_str().unwrap(), "HELLO");
        let value = log.get("value").unwrap();
        match value {
            vector_lib::event::Value::Integer(n) => assert_eq!(*n, 42),
            _ => panic!("Expected integer value"),
        }
    }
}

/// VRL compilation errors in query parameters should fail the build
#[tokio::test]
async fn query_vrl_compilation_error() {
    use crate::config::SourceConfig;
    use vector_lib::source_sender::SourceSender;

    let config = HttpClientConfig {
        endpoint: "http://localhost:9999/endpoint".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::from([(
            "bad_vrl".to_string(),
            QueryParameterValue::SingleParam(ParameterValue::Typed {
                value: "this_function_does_not_exist()".to_string(),
                r#type: ParamType::Vrl,
            }),
        )]),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Get,
        body: None,
        tls: None,
        auth: None,
        log_namespace: None,
    };

    // Attempt to build the source - should fail
    let (tx, _rx) = SourceSender::new_test();
    let cx = crate::config::SourceContext::new_test(tx, None);
    let result = config.build(cx).await;

    // Verify it fails with a VRL compilation error
    match result {
        Err(err) => {
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("VRL compilation failed"),
                "Expected VRL compilation error, got: {}",
                err_msg
            );
        }
        Ok(_) => panic!("Expected build to fail with VRL compilation error, but it succeeded"),
    }
}

/// VRL compilation errors in request body should fail the build
#[tokio::test]
async fn body_vrl_compilation_error() {
    use crate::config::SourceConfig;
    use vector_lib::source_sender::SourceSender;

    let config = HttpClientConfig {
        endpoint: "http://localhost:9999/endpoint".to_string(),
        interval: INTERVAL,
        timeout: TIMEOUT,
        query: HashMap::new(),
        decoding: DeserializerConfig::Json(Default::default()),
        framing: default_framing_message_based(),
        headers: HashMap::new(),
        method: HttpMethod::Post,
        body: Some(ParameterValue::Typed {
            value: "invalid_vrl_syntax((".to_string(),
            r#type: ParamType::Vrl,
        }),
        tls: None,
        auth: None,
        log_namespace: None,
    };

    // Attempt to build the source - should fail
    let (tx, _rx) = SourceSender::new_test();
    let cx = crate::config::SourceContext::new_test(tx, None);
    let result = config.build(cx).await;

    // Verify it fails with a VRL compilation error
    match result {
        Err(err) => {
            let err_msg = err.to_string();
            assert!(
                err_msg.contains("VRL compilation failed"),
                "Expected VRL compilation error, got: {}",
                err_msg
            );
        }
        Ok(_) => panic!("Expected build to fail with VRL compilation error, but it succeeded"),
    }
}
