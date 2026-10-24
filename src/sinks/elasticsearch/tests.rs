use std::{
    collections::{BTreeMap, BTreeSet},
    convert::{Infallible, TryFrom},
    io,
    iter::zip,
    time::Duration,
};

use bytes::Bytes;
use hyper::{
    Body, Request, Response, Server, StatusCode,
    body::HttpBody as _,
    service::{make_service_fn, service_fn},
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinSet,
};
use vector_common::sensitive_string::SensitiveString;
use vector_lib::{
    event::{BatchNotifier, BatchStatus, BatchStatusReceiver, Event},
    lookup::PathPrefix,
};
use vrl::event_path;

use crate::{
    codecs::Transformer,
    config::{ProxyConfig, SinkConfig, SinkContext},
    event::{LogEvent, Metric, MetricKind, MetricValue, ObjectMap, Value},
    sinks::{
        elasticsearch::{
            BulkAction, BulkConfig, DataStreamConfig, Distribution, ElasticsearchApiVersion,
            ElasticsearchAuthConfig, ElasticsearchCommon, ElasticsearchConfig, ElasticsearchMode,
            VersionType, sink::process_log,
        },
        util::{HttpEndpoint, auth::Auth, encoding::Encoder},
    },
    template::{ConfinementConfig, Template, UnconfinedTemplate},
};

// helper to unwrap template strings for tests only
fn parse_template(input: &str) -> Template {
    Template::try_from(input).unwrap()
}

// `bulk.action` / `bulk.version` are unconfined templates.
fn parse_unconfined(input: &str) -> UnconfinedTemplate {
    UnconfinedTemplate::try_from(input).unwrap()
}

type RetryReply = oneshot::Sender<Result<Response<Body>, io::Error>>;

fn retry_bulk_message(body: &[u8]) -> String {
    let mut lines = std::str::from_utf8(body).unwrap().lines();
    let _action = lines.next().unwrap();
    let document: serde_json::Value = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert!(
        lines.next().is_none(),
        "each bulk request must contain one event"
    );
    document["message"].as_str().unwrap().to_owned()
}

async fn assert_retry_backpressure(
    requests: &mut mpsc::UnboundedReceiver<(Bytes, RetryReply)>,
    concurrency: usize,
    expected: &BTreeSet<String>,
    acknowledgements: &mut BTreeMap<String, BatchStatusReceiver>,
) {
    let mut pending = BTreeSet::new();
    let mut original_bodies = BTreeMap::new();
    let mut stalled = Vec::new();

    // Withhold responses until every permitted request has arrived. A retry
    // must retain its original event instead of admitting a new pending batch.
    for failure in 0..4 {
        let mut replies = Vec::new();
        let mut round = BTreeSet::new();
        for _ in 0..concurrency {
            let (body, reply) = requests.recv().await.unwrap();
            let message = retry_bulk_message(&body);
            assert!(expected.contains(&message));
            assert!(round.insert(message.clone()), "concurrent duplicate retry");
            if failure == 0 {
                pending.insert(message.clone());
                original_bodies.insert(message, body);
            } else {
                assert!(
                    pending.contains(&message),
                    "fixed concurrency {concurrency} admitted a new event \
                     while the original requests were still awaiting retries"
                );
                assert_eq!(original_bodies[&message], body);
            }
            replies.push(reply);
        }
        assert_eq!(round, pending);
        for receiver in acknowledgements.values_mut() {
            assert_eq!(
                receiver.try_recv(),
                Err(oneshot::error::TryRecvError::Empty)
            );
        }
        if failure == 3 {
            // Keep the response channels open until the client times out.
            stalled.extend(replies);
            continue;
        }
        for reply in replies {
            let response = match failure {
                0 => Ok(Response::builder()
                    .status(StatusCode::SERVICE_UNAVAILABLE)
                    .body(Body::empty())
                    .unwrap()),
                1 => Ok(Response::builder()
                    .status(StatusCode::TOO_MANY_REQUESTS)
                    .body(Body::empty())
                    .unwrap()),
                _ => Err(io::Error::new(
                    io::ErrorKind::ConnectionReset,
                    "synthetic downstream transport failure",
                )),
            };
            reply.send(response).unwrap();
        }
    }

    // Recover without rebuilding the sink, checking event identities and acks.
    let mut delivered = BTreeSet::new();
    for _ in 0..expected.len() {
        let (body, reply) = requests.recv().await.unwrap();
        let message = retry_bulk_message(&body);
        assert!(expected.contains(&message));
        assert!(
            delivered.insert(message.clone()),
            "duplicate event delivery"
        );
        if pending.remove(&message) {
            assert_eq!(original_bodies[&message], body);
        }
        assert_eq!(
            acknowledgements.get_mut(&message).unwrap().try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        );
        reply
            .send(Ok(Response::new(Body::from(r#"{"errors":false}"#))))
            .unwrap();
    }
    for reply in stalled {
        // The client may have already discarded the timed-out connection.
        drop(reply.send(Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "discarding timed-out synthetic request",
        ))));
    }
    assert!(pending.is_empty());
    assert_eq!(&delivered, expected);
}

#[tokio::test]
async fn fixed_concurrency_bounds_pending_retries_across_endpoints() {
    for concurrency in [1, 2] {
        tokio::time::timeout(Duration::from_secs(30), async {
            let (requests_tx, mut requests) = mpsc::unbounded_channel::<(Bytes, RetryReply)>();
            let mut servers = JoinSet::new();
            let mut endpoints = Vec::new();
            for _ in 0..2 {
                let requests_tx = requests_tx.clone();
                let service = make_service_fn(move |_| {
                    let requests_tx = requests_tx.clone();
                    async move {
                        Ok::<_, Infallible>(service_fn(move |request: Request<Body>| {
                            let requests_tx = requests_tx.clone();
                            async move {
                                assert_eq!(request.uri().path(), "/_bulk");
                                let body = request.into_body().collect().await.unwrap().to_bytes();
                                let (reply, response) = oneshot::channel();
                                requests_tx.send((body, reply)).unwrap();
                                response.await.unwrap()
                            }
                        }))
                    }
                });
                let server = Server::bind(&([127, 0, 0, 1], 0).into());
                endpoints.push(format!("http://{}", server.local_addr()));
                servers.spawn(async move { server.serve(service).await.unwrap() });
            }
            drop(requests_tx);

            let config: ElasticsearchConfig = serde_yaml::from_str(&format!(
                r"
                endpoints: {endpoints:?}
                api_version: v8
                compression: none
                acknowledgements:
                  enabled: true
                batch:
                  max_events: 1
                request:
                  concurrency: {concurrency}
                  timeout_secs: 5
                  retry_initial_backoff_secs: 1
                  retry_max_duration_secs: 1
                  retry_jitter_mode: None
                "
            ))
            .unwrap();
            let (sink, _healthcheck) = config.build(SinkContext::default()).await.unwrap();

            let expected = (0..8)
                .map(|id| format!("retry-backpressure-{id}"))
                .collect::<BTreeSet<_>>();
            let mut acknowledgements = BTreeMap::new();
            let events = expected
                .iter()
                .map(|message| {
                    let (batch, receiver) = BatchNotifier::new_with_receiver();
                    let event =
                        Event::from(LogEvent::from(message.as_str()).with_batch_notifier(&batch));
                    acknowledgements.insert(message.clone(), receiver);
                    event
                })
                .collect::<Vec<_>>();

            let downstream = assert_retry_backpressure(
                &mut requests,
                concurrency,
                &expected,
                &mut acknowledgements,
            );
            let (result, ()) = futures::join!(sink.run_events(events), downstream);
            result.unwrap();
            for receiver in acknowledgements.values_mut() {
                assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
            }
        })
        .await
        .expect("Elasticsearch retry/backpressure test did not complete");
    }
}

#[tokio::test]
async fn sets_create_action_when_configured() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("{{ action }}te"),
            index: parse_template("vector"),
            template_fallback_index: None,
            version: None,
            version_type: VersionType::Internal,
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert(event_path!("action"), "crea");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"vector","_type":"_doc"}}
{"action":"crea","message":"hello there","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encoding_with_external_versioning_without_version_set_does_not_include_version() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("create"),
            template_fallback_index: None,
            index: parse_template("vector"),
            version: None,
            version_type: VersionType::External,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await;
    assert!(es.is_err());
}

#[tokio::test]
async fn encoding_with_external_versioning_with_version_set_includes_version() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("create"),
            index: parse_template("vector"),
            template_fallback_index: None,
            version: Some(parse_unconfined("{{ my_field }}")),
            version_type: VersionType::External,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("config creation failed");

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert(event_path!("my_field"), "1337");
    log.insert(event_path!("my_id"), "42");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, config.id_key.as_ref(), &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_id":"42","_index":"vector","_type":"_doc","version":1337,"version_type":"external"}}
{"message":"hello there","my_field":"1337","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encoding_with_external_gte_versioning_with_version_set_includes_version() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("create"),
            index: parse_template("vector"),
            template_fallback_index: None,
            version: Some(parse_unconfined("{{ my_field }}")),
            version_type: VersionType::ExternalGte,
        },
        id_key: Some("my_id".into()),
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config)
        .await
        .expect("config creation failed");

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert(event_path!("my_field"), "1337");
    log.insert(event_path!("my_id"), "42");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, config.id_key.as_ref(), &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_id":"42","_index":"vector","_type":"_doc","version":1337,"version_type":"external_gte"}}
{"message":"hello there","my_field":"1337","timestamp":"2020-12-01T01:02:03Z"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

fn data_stream_body(
    dtype: Option<String>,
    dataset: Option<String>,
    namespace: Option<String>,
) -> ObjectMap {
    let mut ds = ObjectMap::new();

    if let Some(dtype) = dtype {
        ds.insert("type".into(), Value::from(dtype));
    }

    if let Some(dataset) = dataset {
        ds.insert("dataset".into(), Value::from(dataset));
    }

    if let Some(namespace) = namespace {
        ds.insert("namespace".into(), Value::from(namespace));
    }

    ds
}

fn assert_expected_is_encoded(expected: &str, encoded: &[u8]) {
    let encoded = std::str::from_utf8(encoded).unwrap();

    let expected_lines: Vec<&str> = expected.lines().collect();
    let encoded_lines: Vec<&str> = encoded.lines().collect();

    assert_eq!(expected_lines.len(), encoded_lines.len());

    let to_value = |s: &str| -> serde_json::Value { serde_json::from_str(s).unwrap() };

    zip(expected_lines, encoded_lines).for_each(|(expected, encoded)| {
        assert_eq!(to_value(expected), to_value(encoded));
    });
}

#[tokio::test]
async fn encode_datastream_mode() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        mode: ElasticsearchMode::DataStream,
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    log.insert(
        event_path!("data_stream"),
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-default","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"default","type":"synthetics"},"message":"hello there"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn encode_datastream_mode_no_routing() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        mode: ElasticsearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            auto_routing: false,
            namespace: Template::try_from("something").unwrap(),
            ..Default::default()
        }),
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        event_path!("data_stream"),
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );
    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"logs-generic-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","namespace":"something","type":"synthetics"},"message":"hello there"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn handle_metrics() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("create"),
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let metric = Metric::new(
        "cpu",
        MetricKind::Absolute,
        MetricValue::Gauge { value: 42.0 },
    );
    let log = es.metric_to_log.transform_one(metric).unwrap();

    let mut encoded = vec![];
    es.request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let encoded = std::str::from_utf8(&encoded).unwrap();
    let encoded_lines = encoded.split('\n').map(String::from).collect::<Vec<_>>();
    assert_eq!(encoded_lines.len(), 3); // there's an empty line at the end
    assert_eq!(
        encoded_lines.first().unwrap(),
        r#"{"create":{"_type":"_doc","_index":"vector"}}"#
    );
    assert!(
        encoded_lines
            .get(1)
            .unwrap()
            .starts_with(r#"{"gauge":{"value":42.0},"kind":"absolute","name":"cpu","timestamp""#)
    );
}

#[tokio::test]
async fn decode_bulk_action_error() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("{{ action }}"),
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello world");
    log.insert(event_path!("foo"), "bar");
    log.insert(event_path!("idx"), "purple");
    let action = es.mode.bulk_action(&log);
    assert!(action.is_none());
}

/// validates that the configuration parsing for `ElasticsearchCommon` succeeds when `BulkConfig` is
/// not explicitly set in the configuration (using defaults).
#[tokio::test]
async fn default_bulk_settings() {
    let config = ElasticsearchConfig {
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    assert!(ElasticsearchCommon::parse_single(&config).await.is_ok());
}

#[tokio::test]
async fn decode_bulk_action() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            action: parse_unconfined("create"),
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V7,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let log = LogEvent::from("hello there");
    let action = es.mode.bulk_action(&log).unwrap();
    assert!(matches!(action, BulkAction::Create));
}

#[tokio::test]
async fn encode_datastream_mode_no_sync() {
    use chrono::{TimeZone, Utc};

    use crate::config::log_schema;

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        mode: ElasticsearchMode::DataStream,
        data_stream: Some(DataStreamConfig {
            namespace: Template::try_from("something").unwrap(),
            sync_fields: false,
            ..Default::default()
        }),
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };

    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(
        event_path!("data_stream"),
        data_stream_body(
            Some("synthetics".to_string()),
            Some("testing".to_string()),
            None,
        ),
    );
    log.insert(
        (PathPrefix::Event, log_schema().timestamp_key().unwrap()),
        Utc.with_ymd_and_hms(2020, 12, 1, 1, 2, 3)
            .single()
            .expect("invalid timestamp"),
    );

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"create":{"_index":"synthetics-testing-something","_type":"_doc"}}
{"@timestamp":"2020-12-01T01:02:03Z","data_stream":{"dataset":"testing","type":"synthetics"},"message":"hello there"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn allows_using_except_fields() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        },
        encoding: Transformer::new(None, Some(vec!["idx".into(), "timestamp".into()]), None)
            .unwrap(),
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        // The `{{ idx }}` index has no literal prefix to confine to; opt out so the test can
        // exercise field encoding rather than confinement.
        confinement: ConfinementConfig::unconfined(),
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(event_path!("foo"), "bar");
    log.insert(event_path!("idx"), "purple");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar","message":"hello there"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
async fn allows_using_only_fields() {
    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("{{ idx }}"),
            ..Default::default()
        },
        encoding: Transformer::new(Some(vec!["foo".into()]), None, None).unwrap(),
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        api_version: ElasticsearchApiVersion::V6,
        // The `{{ idx }}` index has no literal prefix to confine to; opt out so the test can
        // exercise field encoding rather than confinement.
        confinement: ConfinementConfig::unconfined(),
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let mut log = LogEvent::from("hello there");
    log.insert(event_path!("foo"), "bar");
    log.insert(event_path!("idx"), "purple");

    let mut encoded = vec![];
    let (encoded_size, _json_size) = es
        .request_builder
        .encoder
        .encode_input(
            vec![process_log(log, &es.mode, None, &config.encoding).unwrap()],
            &mut encoded,
        )
        .unwrap();

    let expected = r#"{"index":{"_index":"purple","_type":"_doc"}}
{"foo":"bar"}
"#;
    assert_expected_is_encoded(expected, &encoded);
    assert_eq!(encoded.len(), encoded_size);
}

#[tokio::test]
// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::too_many_lines,
    reason = "Keep the routing test cases together."
)]
async fn datastream_index_name() {
    #[derive(Clone, Debug)]
    struct TestCase {
        dtype: Option<String>,
        namespace: Option<String>,
        dataset: Option<String>,
        want: String,
    }

    let config = ElasticsearchConfig {
        bulk: BulkConfig {
            index: parse_template("vector"),
            ..Default::default()
        },
        endpoints: vec![HttpEndpoint::parse("https://example.com").unwrap()],
        mode: ElasticsearchMode::DataStream,
        api_version: ElasticsearchApiVersion::V6,
        ..Default::default()
    };
    let es = ElasticsearchCommon::parse_single(&config).await.unwrap();

    let test_cases = [
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "type-dataset-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some(String::new()),
            namespace: Some("namespace".to_string()),
            want: "type-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: None,
            namespace: Some("namespace".to_string()),
            want: "type-generic-namespace".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: Some(String::new()),
            namespace: Some(String::new()),
            want: "type".to_string(),
        },
        TestCase {
            dtype: Some("type".to_string()),
            dataset: None,
            namespace: None,
            want: "type-generic-default".to_string(),
        },
        TestCase {
            dtype: Some(String::new()),
            dataset: Some(String::new()),
            namespace: Some(String::new()),
            want: String::new(),
        },
        TestCase {
            dtype: None,
            dataset: None,
            namespace: None,
            want: "logs-generic-default".to_string(),
        },
        TestCase {
            dtype: Some(String::new()),
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "dataset-namespace".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: Some("dataset".to_string()),
            namespace: Some("namespace".to_string()),
            want: "logs-dataset-namespace".to_string(),
        },
        TestCase {
            dtype: Some(String::new()),
            dataset: Some(String::new()),
            namespace: Some("namespace".to_string()),
            want: "namespace".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: None,
            namespace: Some("namespace".to_string()),
            want: "logs-generic-namespace".to_string(),
        },
        TestCase {
            dtype: Some(String::new()),
            dataset: Some("dataset".to_string()),
            namespace: Some(String::new()),
            want: "dataset".to_string(),
        },
        TestCase {
            dtype: None,
            dataset: Some("dataset".to_string()),
            namespace: None,
            want: "logs-dataset-default".to_string(),
        },
    ];

    for test_case in test_cases {
        let mut log = LogEvent::from("hello there");
        log.insert(
            event_path!("data_stream"),
            data_stream_body(
                test_case.dtype.clone(),
                test_case.dataset.clone(),
                test_case.namespace.clone(),
            ),
        );

        let processed_event = process_log(log, &es.mode, None, &config.encoding).unwrap();
        assert_eq!(processed_event.index, test_case.want, "{test_case:?}");
    }
}

#[tokio::test]
async fn test_parse_config_with_uri_auth() {
    let config = ElasticsearchConfig {
        endpoints: vec![HttpEndpoint::parse("http://user:pass@localhost:9200").unwrap()],
        ..Default::default()
    };
    let proxy = ProxyConfig::default();
    let mut version = None;
    let endpoint = HttpEndpoint::parse("http://user:pass@localhost:9200").unwrap();

    let result = ElasticsearchCommon::parse_config(
        &config,
        Distribution::Elasticsearch,
        &endpoint,
        &proxy,
        &mut version,
    )
    .await;
    assert!(result.is_ok());
    let common = result.unwrap();

    assert!(
        common.auth.is_some(),
        "Expected auth to be the one provided in the uri, got None"
    );

    let expected_auth = crate::http::Auth::Basic {
        user: "user".to_string(),
        password: SensitiveString::from("pass".to_string()),
    };

    let got_auth_inner = match common.auth.as_ref().unwrap() {
        Auth::Basic(auth) => auth,
        #[cfg(feature = "aws-core")]
        Auth::Aws { .. } => panic!("Expected auth to be Basic"),
    };

    assert_eq!(
        *got_auth_inner, expected_auth,
        "Expected auth to be Basic with user 'user' and password 'pass'"
    );
}

#[tokio::test]
async fn test_parse_config_with_config_auth() {
    let config = ElasticsearchConfig {
        auth: Some(ElasticsearchAuthConfig::Basic {
            user: "config_user".to_string(),
            password: SensitiveString::from("config_pass".to_string()),
        }),
        endpoints: vec![HttpEndpoint::parse("http://localhost:9200").unwrap()],
        ..Default::default()
    };
    let proxy = ProxyConfig::default();
    let mut version = None;
    let endpoint = HttpEndpoint::parse("http://localhost:9200").unwrap();

    let result = ElasticsearchCommon::parse_config(
        &config,
        Distribution::Elasticsearch,
        &endpoint,
        &proxy,
        &mut version,
    )
    .await;
    assert!(result.is_ok());
    let common = result.unwrap();

    let expected_auth = crate::http::Auth::Basic {
        user: "config_user".to_string(),
        password: SensitiveString::from("config_pass".to_string()),
    };

    let got_auth_inner = match common.auth.as_ref().unwrap() {
        Auth::Basic(auth) => auth,
        #[cfg(feature = "aws-core")]
        Auth::Aws { .. } => panic!("Expected auth to be Basic"),
    };

    assert_eq!(
        *got_auth_inner, expected_auth,
        "Expected auth to be Basic with user 'user' and password 'pass'"
    );
}

#[tokio::test]
async fn test_parse_config_with_conflicting_auth() {
    let config = ElasticsearchConfig {
        auth: Some(ElasticsearchAuthConfig::Basic {
            user: "config_user".to_string(),
            password: SensitiveString::from("config_pass".to_string()),
        }),
        endpoints: vec![HttpEndpoint::parse("http://uri_user:uri_pass@localhost:9200").unwrap()],
        ..Default::default()
    };
    let proxy = ProxyConfig::default();
    let mut version = None;
    let endpoint = HttpEndpoint::parse("http://uri_user:uri_pass@localhost:9200").unwrap();

    let result = ElasticsearchCommon::parse_config(
        &config,
        Distribution::Elasticsearch,
        &endpoint,
        &proxy,
        &mut version,
    )
    .await;

    // Should fail due to auth being specified in both places
    assert!(result.is_err());
}

#[tokio::test]
async fn api_version_uses_selected_distribution() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use hyper::Body;

    use crate::{sinks::util::test::build_test_server_generic, test_util::addr::next_addr};

    for (distribution, api_version, server_version, expected_version) in [
        (
            Distribution::Elasticsearch,
            ElasticsearchApiVersion::Auto,
            "6.8.23",
            6,
        ),
        (
            Distribution::Elasticsearch,
            ElasticsearchApiVersion::Auto,
            "7.17.0",
            7,
        ),
        (
            Distribution::Elasticsearch,
            ElasticsearchApiVersion::Auto,
            "8.0.0",
            8,
        ),
        (
            Distribution::OpenSearch,
            ElasticsearchApiVersion::Auto,
            "1.3.0",
            7,
        ),
        (
            Distribution::OpenSearch,
            ElasticsearchApiVersion::Auto,
            "2.19.0",
            7,
        ),
        (
            Distribution::OpenSearch,
            ElasticsearchApiVersion::Auto,
            "3.6.0",
            7,
        ),
        (
            Distribution::Elasticsearch,
            ElasticsearchApiVersion::V7,
            "3.6.0",
            7,
        ),
        (
            Distribution::OpenSearch,
            ElasticsearchApiVersion::V8,
            "3.6.0",
            8,
        ),
    ] {
        let (_guard, addr) = next_addr();
        let body = serde_json::json!({"version": {"number": server_version}}).to_string();
        let probes = Arc::new(AtomicUsize::new(0));
        let server_probes = Arc::clone(&probes);
        let (_rx, _trigger, server) = build_test_server_generic(addr, move || {
            server_probes.fetch_add(1, Ordering::Relaxed);
            http::Response::new(Body::from(body.clone()))
        });
        let server = tokio::spawn(server);
        let endpoint = HttpEndpoint::parse(&format!("http://{addr}")).unwrap();
        let config = ElasticsearchConfig {
            endpoints: vec![endpoint.clone()],
            api_version,
            ..Default::default()
        };
        let mut version = None;
        let common = ElasticsearchCommon::parse_config(
            &config,
            distribution,
            &endpoint,
            &ProxyConfig::default(),
            &mut version,
        )
        .await
        .unwrap();
        assert_eq!(
            version,
            Some(expected_version),
            "{distribution:?} with {:?} and server {server_version}",
            config.api_version
        );
        let should_probe = distribution == Distribution::Elasticsearch
            && config.api_version == ElasticsearchApiVersion::Auto;
        assert_eq!(probes.load(Ordering::Relaxed), usize::from(should_probe));
        assert_eq!(
            common.request_builder.encoder.suppress_type_name,
            expected_version >= 7
        );
        server.abort();
    }
}
