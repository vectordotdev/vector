//! Unit tests for the `http` sink.

use std::{
    future::ready,
    sync::{atomic, Arc},
};

use bytes::{Buf, Bytes};
use flate2::{read::MultiGzDecoder, read::ZlibDecoder};
use futures::stream;
use headers::{Authorization, HeaderMapExt};
use hyper::{Body, Method, Response, StatusCode};
use serde::{de, Deserialize};
use vector_lib::codecs::{
    encoding::{Framer, FramingConfig},
    JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig,
};
use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};
use vector_lib::finalization::AddBatchNotifier;

use crate::{
    assert_downcast_matches,
    codecs::{EncodingConfigWithFraming, SinkType},
    log_event,
    sinks::{
        prelude::*,
        util::{
            encoding::Encoder as _,
            http::HeaderValidationError,
            test::{
                build_test_server, build_test_server_generic, build_test_server_status,
                get_received_gzip,
            },
        },
    },
    test_util::{
        components::{
            self, init_test, run_and_assert_sink_compliance, run_and_assert_sink_error_with_events,
            COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS,
        },
        create_events_batch_with_fn, next_addr, random_lines_with_stream,
    },
};

use super::{
    config::HttpSinkConfig,
    config::{validate_headers, validate_payload_wrapper},
    encoder::HttpEncoder,
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<HttpSinkConfig>();
}

fn default_cfg(encoding: EncodingConfigWithFraming) -> HttpSinkConfig {
    HttpSinkConfig {
        uri: Default::default(),
        method: Default::default(),
        auth: Default::default(),
        headers: Default::default(),
        compression: Default::default(),
        encoding,
        payload_prefix: Default::default(),
        payload_suffix: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        tls: Default::default(),
        acknowledgements: Default::default(),
    }
}

#[test]
fn http_encode_event_text() {
    let event = Event::Log(LogEvent::from("hello world"));

    let cfg = default_cfg((None::<FramingConfig>, TextSerializerConfig::default()).into());
    let encoder = cfg.build_encoder().unwrap();
    let transformer = cfg.encoding.transformer();

    let encoder = HttpEncoder::new(encoder, transformer, "".to_owned(), "".to_owned());

    let mut encoded = vec![];
    let (encoded_size, _byte_size) = encoder.encode_input(vec![event], &mut encoded).unwrap();

    assert_eq!(encoded, Vec::from("hello world\n"));
    assert_eq!(encoded.len(), encoded_size);
}

#[test]
fn http_encode_event_ndjson() {
    let event = Event::Log(LogEvent::from("hello world"));

    let cfg = default_cfg(
        (
            Some(NewlineDelimitedEncoderConfig::new()),
            JsonSerializerConfig::default(),
        )
            .into(),
    );
    let encoder = cfg.build_encoder().unwrap();
    let transformer = cfg.encoding.transformer();

    let encoder = HttpEncoder::new(encoder, transformer, "".to_owned(), "".to_owned());

    let mut encoded = vec![];
    encoder.encode_input(vec![event], &mut encoded).unwrap();

    #[derive(Deserialize, Debug)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)] // deserialize all fields
    struct ExpectedEvent {
        message: String,
        timestamp: chrono::DateTime<chrono::Utc>,
    }

    let output = serde_json::from_slice::<ExpectedEvent>(&encoded[..]).unwrap();

    assert_eq!(output.message, "hello world".to_string());
}

#[test]
fn http_validates_normal_headers() {
    let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding.codec = "text"
        [request.headers]
        Auth = "token:thing_and-stuff"
        X-Custom-Nonsense = "_%_{}_-_&_._`_|_~_!_#_&_$_"
        "#;
    let config: HttpSinkConfig = toml::from_str(config).unwrap();

    assert!(validate_headers(&config.request.headers, false).is_ok());
}

#[test]
fn http_catches_bad_header_names() {
    let config = r#"
        uri = "http://$IN_ADDR/frames"
        encoding.codec = "text"
        [request.headers]
        "\u0001" = "bad"
        "#;
    let config: HttpSinkConfig = toml::from_str(config).unwrap();

    assert_downcast_matches!(
        validate_headers(&config.request.headers, false).unwrap_err(),
        HeaderValidationError,
        HeaderValidationError::InvalidHeaderName { .. }
    );
}

#[test]
fn http_validates_payload_prefix_and_suffix() {
    let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = "}"
        "#;
    let config: HttpSinkConfig = toml::from_str(config).unwrap();
    let (framer, serializer) = config.encoding.build(SinkType::MessageBased).unwrap();
    let encoder = Encoder::<Framer>::new(framer, serializer);
    assert!(
        validate_payload_wrapper(&config.payload_prefix, &config.payload_suffix, &encoder).is_ok()
    );
}

#[test]
fn http_validates_payload_prefix_and_suffix_fails_on_invalid_json() {
    let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = ""
        "#;
    let config: HttpSinkConfig = toml::from_str(config).unwrap();
    let (framer, serializer) = config.encoding.build(SinkType::MessageBased).unwrap();
    let encoder = Encoder::<Framer>::new(framer, serializer);
    assert!(
        validate_payload_wrapper(&config.payload_prefix, &config.payload_suffix, &encoder).is_err()
    );
}

// TODO: Fix failure on GH Actions using macos-latest image.
#[cfg(not(target_os = "macos"))]
#[tokio::test]
#[should_panic(expected = "Authorization header can not be used with defined auth options")]
async fn http_headers_auth_conflict() {
    let config = r#"
        uri = "http://$IN_ADDR/"
        encoding.codec = "text"
        [request.headers]
        Authorization = "Basic base64encodedstring"
        [auth]
        strategy = "basic"
        user = "user"
        password = "password"
        "#;
    let config: HttpSinkConfig = toml::from_str(config).unwrap();

    let cx = SinkContext::default();

    _ = config.build(cx).await.unwrap();
}

#[tokio::test]
async fn http_happy_path_post() {
    run_sink(
        r#"
        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#,
        |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
            assert_eq!(
                Some(Authorization::basic("waldo", "hunter2")),
                parts.headers.typed_get()
            );
        },
    )
    .await;
}

#[tokio::test]
async fn http_happy_path_put() {
    run_sink(
        r#"
        method = "put"
        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#,
        |parts| {
            assert_eq!(Method::PUT, parts.method);
            assert_eq!("/frames", parts.uri.path());
            assert_eq!(
                Some(Authorization::basic("waldo", "hunter2")),
                parts.headers.typed_get()
            );
        },
    )
    .await;
}

#[tokio::test]
async fn http_passes_custom_headers() {
    run_sink(
        r#"
        [request.headers]
        foo = "bar"
        baz = "quux"
    "#,
        |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
            assert_eq!(
                Some("bar"),
                parts.headers.get("foo").map(|v| v.to_str().unwrap())
            );
            assert_eq!(
                Some("quux"),
                parts.headers.get("baz").map(|v| v.to_str().unwrap())
            );
        },
    )
    .await;
}

#[tokio::test]
async fn http_passes_template_headers() {
    run_sink_with_events(
        r#"
        [request.headers]
        Static-Header = "static-value"
        Accept = "application/vnd.api+json"
        X-Event-Level = "{{level}}"
        X-Event-Message = "{{message}}"
        X-Static-Template = "constant-value"
    "#,
        || {
            let mut event = Event::Log(LogEvent::from("test message"));
            event.as_mut_log().insert("level", "info");
            event.as_mut_log().insert("message", "templated message");
            event
        },
        10,
        |parts| {
            assert_eq!(
                Some("static-value"),
                parts
                    .headers
                    .get("Static-Header")
                    .map(|v| v.to_str().unwrap())
            );

            assert_eq!(
                Some("application/vnd.api+json"),
                parts.headers.get("Accept").map(|v| v.to_str().unwrap())
            );

            assert_eq!(
                Some("constant-value"),
                parts
                    .headers
                    .get("X-Static-Template")
                    .map(|v| v.to_str().unwrap())
            );

            assert_eq!(
                Some("info"),
                parts
                    .headers
                    .get("X-Event-Level")
                    .map(|v| v.to_str().unwrap())
            );
            assert_eq!(
                Some("templated message"),
                parts
                    .headers
                    .get("X-Event-Message")
                    .map(|v| v.to_str().unwrap())
            );
        },
    )
    .await;
}

#[tokio::test]
async fn http_template_headers_missing_fields() {
    run_sink_with_events(
        r#"
        [request.headers]
        X-Required-Field = "{{required_field}}"
        X-Static = "static-value"
    "#,
        || {
            let mut event = Event::Log(LogEvent::from("good event"));
            event.as_mut_log().insert("required_field", "present");
            event
        },
        10,
        |parts| {
            assert_eq!(
                Some("present"),
                parts
                    .headers
                    .get("X-Required-Field")
                    .map(|v| v.to_str().unwrap())
            );
            assert_eq!(
                Some("static-value"),
                parts.headers.get("X-Static").map(|v| v.to_str().unwrap())
            );
        },
    )
    .await;
}

#[tokio::test]
async fn retries_on_no_connection() {
    components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let num_lines = 10;

        let (in_addr, sink) = build_sink("").await;

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = tokio::spawn(sink.run(events));

        // This ordering starts the sender before the server has built
        // its accepting socket. The delay below ensures that the sink
        // attempts to connect at least once before creating the
        // listening socket.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let (rx, trigger, server) = build_test_server(in_addr);
        tokio::spawn(server);

        pump.await.unwrap().unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received_gzip(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
        })
        .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    })
    .await;
}

#[tokio::test]
async fn retries_on_temporary_error() {
    components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
        const NUM_LINES: usize = 1000;
        const NUM_FAILURES: usize = 2;

        let (in_addr, sink) = build_sink("").await;

        let counter = Arc::new(atomic::AtomicUsize::new(0));
        let in_counter = Arc::clone(&counter);
        let (rx, trigger, server) = build_test_server_generic(in_addr, move || {
            let count = in_counter.fetch_add(1, atomic::Ordering::Relaxed);
            if count < NUM_FAILURES {
                // Send a temporary error for the first two responses
                Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::empty())
                    .unwrap_or_else(|_| unreachable!())
            } else {
                Response::new(Body::empty())
            }
        });

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, NUM_LINES, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = get_received_gzip(rx, |parts| {
            assert_eq!(Method::POST, parts.method);
            assert_eq!("/frames", parts.uri.path());
        })
        .await;

        let tries = counter.load(atomic::Ordering::Relaxed);
        assert!(tries > NUM_FAILURES);
        assert_eq!(NUM_LINES, output_lines.len());
        assert_eq!(input_lines, output_lines);
    })
    .await;
}

#[tokio::test]
async fn fails_on_permanent_error() {
    components::assert_sink_error(&COMPONENT_ERROR_TAGS, async {
        let num_lines = 1000;

        let (in_addr, sink) = build_sink("").await;

        let (rx, trigger, server) = build_test_server_status(in_addr, StatusCode::FORBIDDEN);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (_input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));

        let output_lines =
            get_received_gzip(rx, |_| unreachable!("There should be no lines")).await;
        assert!(output_lines.is_empty());
    })
    .await;
}

#[tokio::test]
async fn json_gzip_compression() {
    json_compression("gzip").await;
}

#[tokio::test]
async fn json_zstd_compression() {
    json_compression("zstd").await;
}

#[tokio::test]
async fn json_zlib_compression() {
    json_compression("zlib").await;
}

#[tokio::test]
async fn json_gzip_compression_with_payload_wrapper() {
    json_compression_with_payload_wrapper("gzip").await;
}

#[tokio::test]
async fn json_zlib_compression_with_payload_wrapper() {
    json_compression_with_payload_wrapper("zlib").await;
}

#[tokio::test]
async fn json_zstd_compression_with_payload_wrapper() {
    json_compression_with_payload_wrapper("zstd").await;
}

async fn json_compression(compression: &str) {
    components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "$COMPRESSION"
        encoding.codec = "json"
        method = "post"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &in_addr.to_string())
        .replace("$COMPRESSION", compression);

        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server(in_addr);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = rx
            .flat_map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );
                let lines: Vec<serde_json::Value> = parse_compressed_json(compression, body);
                stream::iter(lines)
            })
            .map(|line| line.get("message").unwrap().as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    })
    .await;
}

async fn json_compression_with_payload_wrapper(compression: &str) {
    components::assert_sink_compliance(&HTTP_SINK_TAGS, async {
        let num_lines = 1000;

        let in_addr = next_addr();

        let config = r#"
        uri = "http://$IN_ADDR/frames"
        compression = "$COMPRESSION"
        encoding.codec = "json"
        payload_prefix = '{"data":'
        payload_suffix = "}"
        method = "post"

        [auth]
        strategy = "basic"
        user = "waldo"
        password = "hunter2"
    "#
        .replace("$IN_ADDR", &in_addr.to_string())
        .replace("$COMPRESSION", compression);

        let config: HttpSinkConfig = toml::from_str(&config).unwrap();

        let cx = SinkContext::default();

        let (sink, _) = config.build(cx).await.unwrap();
        let (rx, trigger, server) = build_test_server(in_addr);

        let (batch, mut receiver) = BatchNotifier::new_with_receiver();
        let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
        let pump = sink.run(events);

        tokio::spawn(server);

        pump.await.unwrap();
        drop(trigger);

        assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

        let output_lines = rx
            .flat_map(|(parts, body)| {
                assert_eq!(Method::POST, parts.method);
                assert_eq!("/frames", parts.uri.path());
                assert_eq!(
                    Some(Authorization::basic("waldo", "hunter2")),
                    parts.headers.typed_get()
                );

                let message: serde_json::Value = parse_compressed_json(compression, body);

                let lines: Vec<serde_json::Value> = message["data"].as_array().unwrap().to_vec();
                stream::iter(lines)
            })
            .map(|line| line.get("message").unwrap().as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
            .await;

        assert_eq!(num_lines, output_lines.len());
        assert_eq!(input_lines, output_lines);
    })
    .await;
}

#[tokio::test]
async fn templateable_uri_path() {
    init_test();
    fn create_event_with_id(id: i64) -> Event {
        log_event!["id" => id]
    }

    let num_events_per_id = 100;
    let an_id = 1;
    let another_id = 2;
    let in_addr = next_addr();

    let config = format!(
        r#"
        uri = "http://{in_addr}/id/{{{{id}}}}"
        encoding.codec = "json"
        "#
    );

    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    let (rx, trigger, server) = build_test_server(in_addr);

    let (some_events_with_an_id, mut a_receiver) =
        create_events_batch_with_fn(|| create_event_with_id(an_id), num_events_per_id);
    let (some_events_with_another_id, mut another_receiver) =
        create_events_batch_with_fn(|| create_event_with_id(another_id), num_events_per_id);
    let all_events = some_events_with_an_id
        .into_iter()
        .chain(some_events_with_another_id);
    let event_stream = stream::iter(all_events);

    tokio::spawn(server);

    run_and_assert_sink_compliance(sink, event_stream, &HTTP_SINK_TAGS).await;

    drop(trigger);

    assert_eq!(a_receiver.try_recv(), Ok(BatchStatus::Delivered));
    assert_eq!(another_receiver.try_recv(), Ok(BatchStatus::Delivered));

    let request_batches = rx
        .inspect(|(parts, body)| {
            let events: Vec<serde_json::Value> = serde_json::from_slice(body).unwrap();

            // Assert that all the events are received
            assert_eq!(events.len(), num_events_per_id);

            // Assert that all events have the same id
            let expected_event_id = events[0]["id"].as_i64().unwrap();

            for event in events {
                let event_id = event["id"].as_i64().unwrap();
                assert_eq!(event_id, expected_event_id)
            }

            // Assert that the uri path is the expected one for the given id
            let expected_uri_path = format!("/id/{expected_event_id}");
            assert_eq!(parts.uri.path(), expected_uri_path);
        })
        .collect::<Vec<_>>()
        .await;
    assert_eq!(request_batches.len(), 2)
}

#[tokio::test]
async fn templateable_uri_auth() {
    init_test();

    fn create_event_with_user_and_pass(user: &str, pass: &str) -> Event {
        log_event!["user" => user, "pass" => pass]
    }

    let num_events_per_auth = 100;
    let an_user = "an_user";
    let a_pass = "a_pass";
    let another_user = "another_user";
    let another_pass = "another_pass";
    let in_addr = next_addr();
    let config = format!(
        r#"
        uri = "http://{{{{user}}}}:{{{{pass}}}}@{in_addr}/"
        encoding.codec = "json"
        "#
    );

    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    let (rx, trigger, server) = build_test_server(in_addr);

    let (some_events_with_an_auth, mut a_receiver) = create_events_batch_with_fn(
        || create_event_with_user_and_pass(an_user, a_pass),
        num_events_per_auth,
    );
    let (some_events_with_another_auth, mut another_receiver) = create_events_batch_with_fn(
        || create_event_with_user_and_pass(another_user, another_pass),
        num_events_per_auth,
    );
    let all_events = some_events_with_an_auth
        .into_iter()
        .chain(some_events_with_another_auth);
    let event_stream = stream::iter(all_events);

    tokio::spawn(server);

    run_and_assert_sink_compliance(sink, event_stream, &HTTP_SINK_TAGS).await;

    drop(trigger);

    assert_eq!(a_receiver.try_recv(), Ok(BatchStatus::Delivered));
    assert_eq!(another_receiver.try_recv(), Ok(BatchStatus::Delivered));

    let request_batches = rx
        .inspect(|(parts, body)| {
            let events: Vec<serde_json::Value> = serde_json::from_slice(body).unwrap();

            // Assert that all the events are received
            assert_eq!(events.len(), num_events_per_auth);

            // Assert that all events have the same user & pass
            let expected_user = events[0]["user"].as_str().unwrap().to_string();
            let expected_pass = events[0]["pass"].as_str().unwrap().to_string();

            for event in events {
                let event_user = event["user"].as_str().unwrap();
                let event_pass = event["pass"].as_str().unwrap();
                assert_eq!(event_user, expected_user);
                assert_eq!(event_pass, expected_pass);
            }

            // Assert that the auth is the expected one for the given user & pass
            let expected_auth = Authorization::basic(&expected_user, &expected_pass);
            assert_eq!(parts.headers.typed_get(), Some(expected_auth));
        })
        .collect::<Vec<_>>()
        .await;
    assert_eq!(request_batches.len(), 2);
}

#[tokio::test]
async fn missing_field_in_uri_template() {
    init_test();

    let in_addr = next_addr();
    let config = format!(
        r#"
        uri = "http://{in_addr}/{{{{missing_field}}}}"
        encoding.codec = "json"
        "#
    );
    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    let (rx, trigger, server) = build_test_server(in_addr);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut event = Event::Log(LogEvent::default());
    event.add_batch_notifier(batch);

    tokio::spawn(server);
    let expected_emitted_error_events = ["TemplateRenderingError"];
    run_and_assert_sink_error_with_events(
        sink,
        stream::once(ready(event)),
        &expected_emitted_error_events,
        &COMPONENT_ERROR_TAGS,
    )
    .await;

    drop(trigger);

    // TODO(https://github.com/vectordotdev/vector/issues/23366): Currently, When the KeyPartitioner fails to build the batch key from
    // an event, the finalizer is not notified with
    // EventStatus::Rejected. The error is silently ignored.
    // See src/sinks/http/sink.rs:47
    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    // No requests should have been made to the server
    let requests = rx.collect::<Vec<_>>().await;
    assert!(requests.is_empty());
}

#[tokio::test]
async fn http_uri_auth_conflict() {
    init_test();

    let in_addr = next_addr();
    let config = format!(
        r#"
        uri = "http://user:pass@{in_addr}/"
        encoding.codec = "json"
        auth.strategy = "basic"
        auth.user = "user"
        auth.password = "pass"
        "#
    );
    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    let (rx, trigger, server) = build_test_server(in_addr);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let mut event = Event::Log(LogEvent::default());
    event.add_batch_notifier(batch);

    tokio::spawn(server);

    let expected_emitted_error_events = ["ServiceCallError", "SinkRequestBuildError"];
    run_and_assert_sink_error_with_events(
        sink,
        stream::once(ready(event)),
        &expected_emitted_error_events,
        &COMPONENT_ERROR_TAGS,
    )
    .await;

    drop(trigger);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Rejected));

    // No requests should have been made to the server
    let requests = rx.collect::<Vec<_>>().await;
    assert!(requests.is_empty());
}

fn parse_compressed_json<T>(compression: &str, buf: Bytes) -> T
where
    T: de::DeserializeOwned,
{
    match compression {
        "gzip" => serde_json::from_reader(MultiGzDecoder::new(buf.reader())).unwrap(),
        "zstd" => serde_json::from_reader(zstd::Decoder::new(buf.reader()).unwrap()).unwrap(),
        "zlib" => serde_json::from_reader(ZlibDecoder::new(buf.reader())).unwrap(),
        _ => panic!("undefined compression: {compression}"),
    }
}

async fn run_sink(extra_config: &str, assert_parts: impl Fn(http::request::Parts)) {
    let num_lines = 1000;

    let (in_addr, sink) = build_sink(extra_config).await;

    let (rx, trigger, server) = build_test_server(in_addr);
    tokio::spawn(server);

    let (batch, mut receiver) = BatchNotifier::new_with_receiver();
    let (input_lines, events) = random_lines_with_stream(100, num_lines, Some(batch));
    components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
    drop(trigger);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));

    let output_lines = get_received_gzip(rx, assert_parts).await;

    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

async fn run_sink_with_events(
    extra_config: &str,
    event_generator: impl Fn() -> Event + Clone,
    num_events: usize,
    assert_parts: impl Fn(http::request::Parts),
) {
    let (in_addr, sink) = build_sink(extra_config).await;
    let (rx, trigger, server) = build_test_server(in_addr);
    tokio::spawn(server);

    let (events, mut receiver) = create_events_batch_with_fn(event_generator, num_events);
    let events = stream::iter(events);

    components::run_and_assert_sink_compliance(sink, events, &HTTP_SINK_TAGS).await;
    drop(trigger);

    assert_eq!(receiver.try_recv(), Ok(BatchStatus::Delivered));
    let _output_lines = get_received_gzip(rx, assert_parts).await;
}

async fn build_sink(extra_config: &str) -> (std::net::SocketAddr, crate::sinks::VectorSink) {
    let in_addr = next_addr();

    let config = format!(
        r#"
                uri = "http://{in_addr}/frames"
                compression = "gzip"
                framing.method = "newline_delimited"
                encoding.codec = "json"
                {extra_config}
            "#,
    );
    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    (in_addr, sink)
}
