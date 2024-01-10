//! Unit tests for the `http` sink.

use std::{
    io::{BufRead, BufReader},
    sync::{atomic, Arc},
};

use bytes::{Buf, Bytes};
use flate2::{read::MultiGzDecoder, read::ZlibDecoder};
use futures::{channel::mpsc, stream};
use headers::{Authorization, HeaderMapExt};
use http::request::Parts;
use hyper::{Body, Method, Response, StatusCode};
use serde::{de, Deserialize};
use vector_lib::codecs::{
    encoding::{Framer, FramingConfig},
    JsonSerializerConfig, NewlineDelimitedEncoderConfig, TextSerializerConfig,
};

use vector_lib::event::{BatchNotifier, BatchStatus, Event, LogEvent};

use crate::{
    assert_downcast_matches,
    codecs::{EncodingConfigWithFraming, SinkType},
    sinks::{
        prelude::*,
        util::{
            encoding::Encoder as _,
            http::HeaderValidationError,
            test::{build_test_server, build_test_server_generic, build_test_server_status},
        },
    },
    test_util::{
        components,
        components::{COMPONENT_ERROR_TAGS, HTTP_SINK_TAGS},
        next_addr, random_lines_with_stream,
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

        let output_lines = get_received(rx, |parts| {
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

        let output_lines = get_received(rx, |parts| {
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

        let output_lines = get_received(rx, |_| unreachable!("There should be no lines")).await;
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

fn parse_compressed_json<T>(compression: &str, buf: Bytes) -> T
where
    T: de::DeserializeOwned,
{
    match compression {
        "gzip" => serde_json::from_reader(MultiGzDecoder::new(buf.reader())).unwrap(),
        "zstd" => serde_json::from_reader(zstd::Decoder::new(buf.reader()).unwrap()).unwrap(),
        "zlib" => serde_json::from_reader(ZlibDecoder::new(buf.reader())).unwrap(),
        _ => panic!("undefined compression: {}", compression),
    }
}

async fn get_received(
    rx: mpsc::Receiver<(Parts, Bytes)>,
    assert_parts: impl Fn(Parts),
) -> Vec<String> {
    rx.flat_map(|(parts, body)| {
        assert_parts(parts);
        stream::iter(BufReader::new(MultiGzDecoder::new(body.reader())).lines())
    })
    .map(Result::unwrap)
    .map(|line| {
        let val: serde_json::Value = serde_json::from_str(&line).unwrap();
        val.get("message").unwrap().as_str().unwrap().to_owned()
    })
    .collect::<Vec<_>>()
    .await
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

    let output_lines = get_received(rx, assert_parts).await;

    assert_eq!(num_lines, output_lines.len());
    assert_eq!(input_lines, output_lines);
}

async fn build_sink(extra_config: &str) -> (std::net::SocketAddr, crate::sinks::VectorSink) {
    let in_addr = next_addr();

    let config = format!(
        r#"
                uri = "http://{addr}/frames"
                compression = "gzip"
                framing.method = "newline_delimited"
                encoding.codec = "json"
                {extras}
            "#,
        addr = in_addr,
        extras = extra_config,
    );
    let config: HttpSinkConfig = toml::from_str(&config).unwrap();

    let cx = SinkContext::default();

    let (sink, _) = config.build(cx).await.unwrap();
    (in_addr, sink)
}
