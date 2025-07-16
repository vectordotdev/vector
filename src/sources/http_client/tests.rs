use http::Uri;
use std::collections::HashMap;
use tokio::time::Duration;
use vector_lib::config::LogNamespace;
use warp::{http::HeaderMap, Filter};

use crate::components::validation::prelude::*;
use crate::http::{ParamType, ParameterValue, QueryParameterValue};
use crate::sources::util::http::HttpMethod;
use crate::{serde::default_decoding, serde::default_framing_message_based};
use vector_lib::codecs::decoding::{
    CharacterDelimitedDecoderOptions, DeserializerConfig, FramingConfig,
};
use vector_lib::codecs::CharacterDelimitedDecoderConfig;
use vector_lib::event::Event;

use super::HttpClientConfig;
use crate::test_util::{
    components::{run_and_assert_source_compliance, HTTP_PULL_SOURCE_TAGS},
    next_addr, test_generate_config, wait_for_tcp,
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
    let in_addr = next_addr();

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
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// JSON with newline delimiter should be decoded and HTTP header set to application/x-ndjson.
#[tokio::test]
async fn json_decoding_newline_delimited() {
    let in_addr = next_addr();

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
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// JSON with character delimiter should be decoded and HTTP header set to application/json.
#[tokio::test]
async fn json_decoding_character_delimited() {
    let in_addr = next_addr();

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
        tls: None,
        auth: None,
        log_namespace: None,
    })
    .await;
}

/// HTTP request queries configured by the user should be applied correctly.
#[tokio::test]
async fn request_query_applied() {
    let in_addr = next_addr();

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
    let in_addr = next_addr();

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
    let in_addr = next_addr();

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
    let in_addr = next_addr();

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
        auth: None,
        tls: None,
        log_namespace: None,
    })
    .await;
}

/// ACCEPT HTTP request headers configured by the user should take precedence
#[tokio::test]
async fn accept_header_override() {
    let in_addr = next_addr();

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
        auth: None,
        tls: None,
        log_namespace: None,
    })
    .await;
}
