use std::{net::SocketAddr, num::NonZeroU64};

use chrono::{TimeZone, Utc};
use futures_util::Stream;
use http::Uri;
use reqwest::{RequestBuilder, Response};
use serde::Deserialize;
use vector_lib::{
    codecs::{
        BytesDecoderConfig, JsonSerializerConfig, TextSerializerConfig,
        decoding::{
            DeserializerConfig,
            format::{VrlDeserializerConfig, VrlDeserializerOptions},
        },
    },
    event::EventStatus,
    schema::Definition,
    sensitive_string::SensitiveString,
};
use vrl::path::PathPrefix;

use super::*;
use crate::{
    SourceSender,
    codecs::{DecodingConfig, EncodingConfig},
    components::validation::prelude::*,
    config::{SinkConfig, SinkContext, SourceConfig, SourceContext, log_schema},
    event::{Event, LogEvent},
    sinks::{
        Healthcheck, VectorSink,
        splunk_hec::logs::config::HecLogsSinkConfig,
        util::{BatchConfig, Compression, HttpEndpoint, TowerRequestConfig},
    },
    sources::splunk_hec::acknowledgements::{HecAckStatusRequest, HecAckStatusResponse},
    test_util::{
        addr::{PortGuard, next_addr},
        collect_n,
        components::{
            COMPONENT_ERROR_TAGS, HTTP_PUSH_SOURCE_TAGS, assert_source_compliance,
            assert_source_error,
        },
        wait_for_tcp,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<SplunkConfig>();
}

#[tokio::test]
async fn finish_err_maps_capped_body_to_client_error() {
    // `capped_body()` rejects oversized payloads with an `ErrorMessage` (e.g. 413). The
    // recovery must surface that status instead of letting it fall through to a 500.
    let rejection = warp::reject::custom(ErrorMessage::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "Request body exceeds limit of 1024 bytes.".to_owned(),
    ));

    let (response,) = finish_err(rejection)
        .await
        .expect("capped-body rejection should be recovered");

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

/// Splunk token
const TOKEN: &str = "token";
const VALID_TOKENS: &[&str; 2] = &[TOKEN, "secondary-token"];

async fn source(
    acknowledgements: Option<HecAcknowledgementsConfig>,
) -> (impl Stream<Item = Event> + Unpin, SocketAddr, PortGuard) {
    source_with(Some(TOKEN.to_owned().into()), None, acknowledgements, false).await
}

async fn source_with(
    token: Option<SensitiveString>,
    valid_tokens: Option<&[&str]>,
    acknowledgements: Option<HecAcknowledgementsConfig>,
    store_hec_token: bool,
) -> (
    impl Stream<Item = Event> + Unpin + use<>,
    SocketAddr,
    PortGuard,
) {
    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let (_guard, address) = next_addr();
    let valid_tokens =
        valid_tokens.map(|tokens| tokens.iter().map(|v| v.to_string().into()).collect());
    let cx = SourceContext::new_test(sender, None);
    tokio::spawn(async move {
        SplunkConfig {
            address,
            token,
            valid_tokens,
            tls: None,
            acknowledgements: acknowledgements.unwrap_or_default(),
            store_hec_token,
            log_namespace: None,
            keepalive: Default::default(),
            event: CodecConfig::default(),
            raw: CodecConfig::default(),
        }
        .build(cx)
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;
    (recv, address, _guard)
}

async fn sink(
    address: SocketAddr,
    encoding: EncodingConfig,
    compression: Compression,
) -> (VectorSink, Healthcheck) {
    HecLogsSinkConfig {
        default_token: TOKEN.to_owned().into(),
        endpoint: HttpEndpoint::parse(&format!("http://{address}")).unwrap(),
        host_key: None,
        indexed_fields: vec![],
        index: None,
        sourcetype: None,
        source: None,
        encoding,
        compression,
        batch: BatchConfig::default(),
        request: TowerRequestConfig::default(),
        tls: None,
        acknowledgements: Default::default(),
        timestamp_nanos_key: None,
        timestamp_key: None,
        auto_extract_timestamp: None,
        endpoint_target: Default::default(),
        confinement: Default::default(),
    }
    .build(SinkContext::default())
    .await
    .unwrap()
}

async fn start(
    encoding: EncodingConfig,
    compression: Compression,
    acknowledgements: Option<HecAcknowledgementsConfig>,
) -> (VectorSink, impl Stream<Item = Event> + Unpin) {
    let (source, address, _guard) = source(acknowledgements).await;
    let (sink, health) = sink(address, encoding, compression).await;
    assert!(health.await.is_ok());
    (sink, source)
}

async fn channel_n(
    messages: Vec<impl Into<String> + Send + 'static>,
    sink: VectorSink,
    source: impl Stream<Item = Event> + Unpin,
) -> Vec<Event> {
    let n = messages.len();

    tokio::spawn(async move {
        sink.run_events(
            messages
                .into_iter()
                .map(|s| Event::Log(LogEvent::from(s.into()))),
        )
        .await
        .unwrap();
    });

    let events = collect_n(source, n).await;
    assert_eq!(n, events.len());

    events
}

#[derive(Clone, Copy, Debug)]
enum Channel<'a> {
    Header(&'a str),
    QueryParam(&'a str),
}

#[derive(Default)]
struct SendWithOpts<'a> {
    channel: Option<Channel<'a>>,
    forwarded_for: Option<String>,
}

async fn post(address: SocketAddr, api: &str, message: &str) -> u16 {
    let channel = Channel::Header("channel");
    let options = SendWithOpts {
        channel: Some(channel),
        forwarded_for: None,
    };
    send_with(address, api, message, TOKEN, &options).await
}

fn build_request(
    address: SocketAddr,
    api: &str,
    message: &str,
    token: &str,
    opts: &SendWithOpts<'_>,
) -> RequestBuilder {
    let mut b = reqwest::Client::new()
        .post(format!("http://{address}/{api}"))
        .header("Authorization", format!("Splunk {token}"));

    b = match opts.channel {
        Some(c) => match c {
            Channel::Header(v) => b.header("x-splunk-request-channel", v),
            Channel::QueryParam(v) => b.query(&[("channel", v)]),
        },
        None => b,
    };

    b = match &opts.forwarded_for {
        Some(f) => b.header("X-Forwarded-For", f),
        None => b,
    };

    b.body(message.to_owned())
}

async fn send_with(
    address: SocketAddr,
    api: &str,
    message: &str,
    token: &str,
    opts: &SendWithOpts<'_>,
) -> u16 {
    let b = build_request(address, api, message, token, opts);
    b.send().await.unwrap().status().as_u16()
}

async fn send_with_response(
    address: SocketAddr,
    api: &str,
    message: &str,
    token: &str,
    opts: &SendWithOpts<'_>,
) -> Response {
    let b = build_request(address, api, message, token, opts);
    b.send().await.unwrap()
}

#[tokio::test]
async fn no_compression_text_event() {
    let message = "gzip_text_event";
    let (sink, source) = start(
        TextSerializerConfig::default().into(),
        Compression::None,
        None,
    )
    .await;

    let event = channel_n(vec![message], sink, source).await.remove(0);

    assert_eq!(
        event.as_log()[log_schema().message_key().unwrap().to_string()],
        message.into()
    );
    assert!(event.as_log().get_timestamp().is_some());
    assert_eq!(
        event.as_log()[log_schema().source_type_key().unwrap().to_string()],
        "splunk_hec".into()
    );
    assert!(event.metadata().splunk_hec_token().is_none());
}

#[tokio::test]
async fn one_simple_text_event() {
    let message = "one_simple_text_event";
    let (sink, source) = start(
        TextSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let event = channel_n(vec![message], sink, source).await.remove(0);

    assert_eq!(
        event.as_log()[log_schema().message_key().unwrap().to_string()],
        message.into()
    );
    assert!(event.as_log().get_timestamp().is_some());
    assert_eq!(
        event.as_log()[log_schema().source_type_key().unwrap().to_string()],
        "splunk_hec".into()
    );
    assert!(event.metadata().splunk_hec_token().is_none());
}

#[tokio::test]
async fn multiple_simple_text_event() {
    let n = 200;
    let (sink, source) = start(
        TextSerializerConfig::default().into(),
        Compression::None,
        None,
    )
    .await;

    let messages = (0..n)
        .map(|i| format!("multiple_simple_text_event_{i}"))
        .collect::<Vec<_>>();
    let events = channel_n(messages.clone(), sink, source).await;

    for (msg, event) in messages.into_iter().zip(events.into_iter()) {
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            msg.into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }
}

#[tokio::test]
async fn one_simple_json_event() {
    let message = "one_simple_json_event";
    let (sink, source) = start(
        JsonSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let event = channel_n(vec![message], sink, source).await.remove(0);

    assert_eq!(
        event.as_log()[log_schema().message_key().unwrap().to_string()],
        message.into()
    );
    assert!(event.as_log().get_timestamp().is_some());
    assert_eq!(
        event.as_log()[log_schema().source_type_key().unwrap().to_string()],
        "splunk_hec".into()
    );
    assert!(event.metadata().splunk_hec_token().is_none());
}

#[tokio::test]
async fn multiple_simple_json_event() {
    let n = 200;
    let (sink, source) = start(
        JsonSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let messages = (0..n)
        .map(|i| format!("multiple_simple_json_event{i}"))
        .collect::<Vec<_>>();
    let events = channel_n(messages.clone(), sink, source).await;

    for (msg, event) in messages.into_iter().zip(events.into_iter()) {
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            msg.into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    }
}

#[tokio::test]
async fn json_event() {
    let (sink, source) = start(
        JsonSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let mut log = LogEvent::default();
    log.insert(event_path!("greeting"), "hello");
    log.insert(event_path!("name"), "bob");
    sink.run_events(vec![log.into()]).await.unwrap();

    let event = collect_n(source, 1).await.remove(0).into_log();
    assert_eq!(event["greeting"], "hello".into());
    assert_eq!(event["name"], "bob".into());
    assert!(event.get_timestamp().is_some());
    assert_eq!(
        event[log_schema().source_type_key().unwrap().to_string()],
        "splunk_hec".into()
    );
    assert!(event.metadata().splunk_hec_token().is_none());
}

#[tokio::test]
async fn json_invalid_path_event() {
    let (sink, source) = start(
        JsonSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let mut log = LogEvent::default();
    // Test with a field that would be considered an invalid path if it were to
    // be treated as a path and not a simple field name.
    log.insert(event_path!("(greeting | thing"), "hello");
    sink.run_events(vec![log.into()]).await.unwrap();

    let event = collect_n(source, 1).await.remove(0).into_log();
    assert_eq!(
        event.get(event_path!("(greeting | thing")),
        Some(&Value::from("hello"))
    );
}

#[tokio::test]
async fn line_to_message() {
    let (sink, source) = start(
        JsonSerializerConfig::default().into(),
        Compression::gzip_default(),
        None,
    )
    .await;

    let mut event = LogEvent::default();
    event.insert(event_path!("line"), "hello");
    sink.run_events(vec![event.into()]).await.unwrap();

    let event = collect_n(source, 1).await.remove(0);
    assert_eq!(
        event.as_log()[log_schema().message_key().unwrap().to_string()],
        "hello".into()
    );
    assert!(event.metadata().splunk_hec_token().is_none());
}

#[tokio::test]
async fn raw() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "raw";
        let (source, address, _guard) = source(None).await;

        assert_eq!(200, post(address, "services/collector/raw", message).await);

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    })
    .await;
}

#[tokio::test]
async fn root() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{ "event": { "message": "root"} }"#;
        let (source, address, _guard) = source(None).await;

        assert_eq!(200, post(address, "services/collector", message).await);

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            "root".into()
        );
        assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    })
    .await;
}

#[tokio::test]
async fn channel_header() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "raw";
        let (source, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
    })
    .await;
}

#[tokio::test]
async fn xff_header_raw() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "raw";
        let (source, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
            "10.0.0.1".into()
        );
    })
    .await;
}

// Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
#[tokio::test]
async fn xff_header_event_with_host_field() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{"event":"first", "host": "10.1.0.2"}"#;
        let (source, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
            "10.1.0.2".into()
        );
    })
    .await;
}

// Test helps to illustrate that a payload's `host` value should override an x-forwarded-for header
#[tokio::test]
async fn xff_header_event_without_host_field() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let (source, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::Header("guid")),
            forwarded_for: Some(String::from("10.0.0.1")),
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/event", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().host_key().unwrap().to_string().as_str()],
            "10.0.0.1".into()
        );
    })
    .await;
}

#[tokio::test]
async fn channel_query_param() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "raw";
        let (source, address, _guard) = source(None).await;

        let opts = SendWithOpts {
            channel: Some(Channel::QueryParam("guid")),
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(address, "services/collector/raw", message, TOKEN, &opts).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(event.as_log()[&super::CHANNEL], "guid".into());
    })
    .await;
}

#[tokio::test]
async fn no_data() {
    let (_source, address, _guard) = source(None).await;

    assert_eq!(400, post(address, "services/collector/event", "").await);
}

#[tokio::test]
async fn invalid_token() {
    assert_source_error(&COMPONENT_ERROR_TAGS, async {
        let (_source, address, _guard) = source(None).await;
        let opts = SendWithOpts {
            channel: Some(Channel::Header("channel")),
            forwarded_for: None,
        };

        assert_eq!(
            401,
            send_with(address, "services/collector/event", "", "nope", &opts).await
        );
    })
    .await;
}

#[tokio::test]
async fn health_ignores_token() {
    let (_source, address, _guard) = source(None).await;

    let res = reqwest::Client::new()
        .get(format!("http://{address}/services/collector/health"))
        .header("Authorization", format!("Splunk {}", "invalid token"))
        .send()
        .await
        .unwrap();

    assert_eq!(200, res.status().as_u16());
}

#[tokio::test]
async fn health() {
    let (_source, address, _guard) = source(None).await;

    let res = reqwest::Client::new()
        .get(format!("http://{address}/services/collector/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(200, res.status().as_u16());
}

#[tokio::test]
async fn secondary_token() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{"event":"first", "color": "blue"}"#;
        let (_source, address, _guard) = source_with(None, Some(VALID_TOKENS), None, false).await;
        let options = SendWithOpts {
            channel: None,
            forwarded_for: None,
        };

        assert_eq!(
            200,
            send_with(
                address,
                "services/collector/event",
                message,
                VALID_TOKENS.get(1).unwrap(),
                &options
            )
            .await
        );
    })
    .await;
}

#[tokio::test]
async fn event_service_token_passthrough_enabled() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "passthrough_token_enabled";
        let (source, address, _guard) = source_with(None, Some(VALID_TOKENS), None, true).await;
        let (sink, health) = sink(
            address,
            TextSerializerConfig::default().into(),
            Compression::gzip_default(),
        )
        .await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert_eq!(
            event.metadata().splunk_hec_token().as_deref().unwrap(),
            TOKEN
        );
    })
    .await;
}

#[tokio::test]
async fn raw_service_token_passthrough_enabled() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "raw";
        let (source, address, _guard) = source_with(None, Some(VALID_TOKENS), None, true).await;

        assert_eq!(200, post(address, "services/collector/raw", message).await);

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
        assert_eq!(
            event.metadata().splunk_hec_token().as_deref().unwrap(),
            TOKEN
        );
    })
    .await;
}

#[tokio::test]
async fn no_authorization() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "no_authorization";
        let (source, address, _guard) = source_with(None, None, None, false).await;
        let (sink, health) = sink(
            address,
            TextSerializerConfig::default().into(),
            Compression::gzip_default(),
        )
        .await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert!(event.metadata().splunk_hec_token().is_none());
    })
    .await;
}

#[tokio::test]
async fn no_authorization_token_passthrough_enabled() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "no_authorization";
        let (source, address, _guard) = source_with(None, None, None, true).await;
        let (sink, health) = sink(
            address,
            TextSerializerConfig::default().into(),
            Compression::gzip_default(),
        )
        .await;
        assert!(health.await.is_ok());

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert_eq!(
            event.metadata().splunk_hec_token().as_deref().unwrap(),
            TOKEN
        );
    })
    .await;
}

#[tokio::test]
async fn partial() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"{"event":"first"}{"event":"second""#;
        let (source, address, _guard) = source(None).await;

        assert_eq!(
            400,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            "first".into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
    })
    .await;
}

#[tokio::test]
async fn handles_newlines() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#"
{"event":"first"}
    "#;
        let (source, address, _guard) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            "first".into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
    })
    .await;
}

#[tokio::test]
async fn handles_spaces() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = r#" {"event":"first"} "#;
        let (source, address, _guard) = source(None).await;

        assert_eq!(
            200,
            post(address, "services/collector/event", message).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            "first".into()
        );
        assert!(event.as_log().get_timestamp().is_some());
        assert_eq!(
            event.as_log()[log_schema().source_type_key().unwrap().to_string()],
            "splunk_hec".into()
        );
    })
    .await;
}

#[tokio::test]
async fn handles_non_utf8() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
    let message = b" {\"event\": { \"non\": \"A non UTF8 character \xE4\", \"number\": 2, \"bool\": true } } ";
    let (source, address, _guard) = source(None).await;

    let b = reqwest::Client::new()
        .post(format!(
            "http://{}/{}",
            address, "services/collector/event"
        ))
        .header("Authorization", format!("Splunk {TOKEN}"))
        .body::<&[u8]>(message);

    assert_eq!(200, b.send().await.unwrap().status().as_u16());

    let event = collect_n(source, 1).await.remove(0);
    assert_eq!(event.as_log()["non"], "A non UTF8 character �".into());
    assert_eq!(event.as_log()["number"], 2.into());
    assert_eq!(event.as_log()["bool"], true.into());
    assert!(event.as_log().get((lookup::PathPrefix::Event, log_schema().timestamp_key().unwrap())).is_some());
    assert_eq!(
        event.as_log()[log_schema().source_type_key().unwrap().to_string()],
        "splunk_hec".into()
    );
}).await;
}

#[tokio::test]
async fn default() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
    let message = r#"{"event":"first","source":"main"}{"event":"second"}{"event":"third","source":"secondary"}"#;
    let (source, address, _guard) = source(None).await;

    assert_eq!(
        200,
        post(address, "services/collector/event", message).await
    );

    let events = collect_n(source, 3).await;

    assert_eq!(
        events[0].as_log()[log_schema().message_key().unwrap().to_string()],
        "first".into()
    );
    assert_eq!(events[0].as_log()[&super::SOURCE], "main".into());

    assert_eq!(
        events[1].as_log()[log_schema().message_key().unwrap().to_string()],
        "second".into()
    );
    assert_eq!(events[1].as_log()[&super::SOURCE], "main".into());

    assert_eq!(
        events[2].as_log()[log_schema().message_key().unwrap().to_string()],
        "third".into()
    );
    assert_eq!(events[2].as_log()[&super::SOURCE], "secondary".into());
}).await;
}

#[test]
fn parse_timestamps() {
    let cases = vec![
        Utc::now(),
        Utc.with_ymd_and_hms(1971, 11, 7, 1, 1, 1)
            .single()
            .expect("invalid timestamp"),
        Utc.with_ymd_and_hms(2011, 8, 5, 1, 1, 1)
            .single()
            .expect("invalid timestamp"),
        Utc.with_ymd_and_hms(2189, 11, 4, 2, 2, 2)
            .single()
            .expect("invalid timestamp"),
    ];

    for case in cases {
        let sec = case.timestamp();
        let millis = case.timestamp_millis();
        let nano = case.timestamp_nanos_opt().expect("Timestamp out of range");

        assert_eq!(parse_timestamp(sec).unwrap().timestamp(), case.timestamp());
        assert_eq!(
            parse_timestamp(millis).unwrap().timestamp_millis(),
            case.timestamp_millis()
        );
        assert_eq!(
            parse_timestamp(nano)
                .unwrap()
                .timestamp_nanos_opt()
                .unwrap(),
            case.timestamp_nanos_opt().expect("Timestamp out of range")
        );
    }

    assert!(parse_timestamp(-1).is_none());
}

/// This test will fail once `warp` crate fixes support for
/// custom connection listener, at that point this test can be
/// modified to pass.
/// https://github.com/vectordotdev/vector/issues/7097
/// https://github.com/seanmonstar/warp/issues/830
/// https://github.com/seanmonstar/warp/pull/713
#[tokio::test]
async fn host_test() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let message = "for the host";
        let (sink, source) = start(
            TextSerializerConfig::default().into(),
            Compression::gzip_default(),
            None,
        )
        .await;

        let event = channel_n(vec![message], sink, source).await.remove(0);

        assert_eq!(
            event.as_log()[log_schema().message_key().unwrap().to_string()],
            message.into()
        );
        assert!(
            event
                .as_log()
                .get((PathPrefix::Event, log_schema().host_key().unwrap()))
                .is_none()
        );
    })
    .await;
}

#[derive(Deserialize)]
struct HecAckEventResponse {
    text: String,
    code: u8,
    #[serde(rename = "ackId")]
    ack_id: u64,
}

#[tokio::test]
async fn ack_json_event() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (source, address, _guard) = source(Some(ack_config)).await;
    let event_message = r#"{"event":"first", "color": "blue"}{"event":"second"}"#;
    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    let event_res = send_with_response(
        address,
        "services/collector/event",
        event_message,
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckEventResponse>()
    .await
    .unwrap();
    assert_eq!("Success", event_res.text.as_str());
    assert_eq!(0, event_res.code);
    _ = collect_n(source, 1).await;

    let ack_message = serde_json::to_string(&HecAckStatusRequest {
        acks: vec![event_res.ack_id],
    })
    .unwrap();
    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
}

#[tokio::test]
async fn ack_raw_event() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (source, address, _guard) = source(Some(ack_config)).await;
    let event_message = "raw event message";
    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    let event_res = send_with_response(
        address,
        "services/collector/raw",
        event_message,
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckEventResponse>()
    .await
    .unwrap();
    assert_eq!("Success", event_res.text.as_str());
    assert_eq!(0, event_res.code);
    _ = collect_n(source, 1).await;

    let ack_message = serde_json::to_string(&HecAckStatusRequest {
        acks: vec![event_res.ack_id],
    })
    .unwrap();
    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
}

#[tokio::test]
async fn ack_repeat_ack_query() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (source, address, _guard) = source(Some(ack_config)).await;
    let event_message = "raw event message";
    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    let event_res = send_with_response(
        address,
        "services/collector/raw",
        event_message,
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckEventResponse>()
    .await
    .unwrap();
    _ = collect_n(source, 1).await;

    let ack_message = serde_json::to_string(&HecAckStatusRequest {
        acks: vec![event_res.ack_id],
    })
    .unwrap();
    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(ack_res.acks.get(&event_res.ack_id).unwrap());

    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(!ack_res.acks.get(&event_res.ack_id).unwrap());
}

#[tokio::test]
async fn ack_exceed_max_number_of_ack_channels() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        max_number_of_ack_channels: NonZeroU64::new(1).unwrap(),
        ..Default::default()
    };

    let (_source, address, _guard) = source(Some(ack_config)).await;
    let mut opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    assert_eq!(
        200,
        send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
    );

    opts.channel = Some(Channel::Header("other-guid"));
    assert_eq!(
        503,
        send_with(address, "services/collector/raw", "message", TOKEN, &opts).await
    );
    assert_eq!(
        503,
        send_with(
            address,
            "services/collector/event",
            r#"{"event":"first"}"#,
            TOKEN,
            &opts
        )
        .await
    );
}

#[tokio::test]
async fn ack_exceed_max_pending_acks_per_channel() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        max_pending_acks_per_channel: NonZeroU64::new(1).unwrap(),
        ..Default::default()
    };

    let (source, address, _guard) = source(Some(ack_config)).await;
    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    for _ in 0..5 {
        send_with(
            address,
            "services/collector/event",
            r#"{"event":"first"}"#,
            TOKEN,
            &opts,
        )
        .await;
    }
    for _ in 0..5 {
        send_with(address, "services/collector/raw", "message", TOKEN, &opts).await;
    }
    let event_res = send_with_response(
        address,
        "services/collector/event",
        r#"{"event":"this will be acked"}"#,
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckEventResponse>()
    .await
    .unwrap();
    _ = collect_n(source, 11).await;

    let ack_message_dropped = serde_json::to_string(&HecAckStatusRequest {
        acks: (0..10).collect::<Vec<u64>>(),
    })
    .unwrap();
    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message_dropped.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(ack_res.acks.values().all(|ack_status| !*ack_status));

    let ack_message_acked = serde_json::to_string(&HecAckStatusRequest {
        acks: vec![event_res.ack_id],
    })
    .unwrap();
    let ack_res = send_with_response(
        address,
        "services/collector/ack",
        ack_message_acked.as_str(),
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckStatusResponse>()
    .await
    .unwrap();
    assert!(ack_res.acks.get(&event_res.ack_id).unwrap());
}

#[tokio::test]
async fn ack_service_accepts_parameterized_content_type() {
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (source, address, _guard) = source(Some(ack_config)).await;
    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };

    let event_res = send_with_response(
        address,
        "services/collector/event",
        r#"{"event":"param-test"}"#,
        TOKEN,
        &opts,
    )
    .await
    .json::<HecAckEventResponse>()
    .await
    .unwrap();
    let _ = collect_n(source, 1).await;

    let body = serde_json::to_string(&HecAckStatusRequest {
        acks: vec![event_res.ack_id],
    })
    .unwrap();

    let res = reqwest::Client::new()
        .post(format!("http://{address}/services/collector/ack"))
        .header("Authorization", format!("Splunk {TOKEN}"))
        .header("x-splunk-request-channel", "guid")
        .header("Content-Type", "application/json; some-random-text; hello")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(200, res.status().as_u16());

    let _parsed: HecAckStatusResponse = res.json().await.unwrap();
}

#[tokio::test]
async fn event_service_acknowledgements_enabled_channel_required() {
    let message = r#"{"event":"first", "color": "blue"}"#;
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (_, address, _guard) = source(Some(ack_config)).await;

    let opts = SendWithOpts {
        channel: None,
        forwarded_for: None,
    };

    assert_eq!(
        400,
        send_with(address, "services/collector/event", message, TOKEN, &opts).await
    );
}

#[tokio::test]
async fn ack_service_acknowledgements_disabled() {
    let message = r#" {"acks":[0]} "#;
    let (_, address, _guard) = source(None).await;

    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };

    assert_eq!(
        400,
        send_with(address, "services/collector/ack", message, TOKEN, &opts).await
    );
}

async fn source_with_codec(
    event: CodecConfig,
    raw: CodecConfig,
) -> (
    impl Stream<Item = Event> + Unpin + use<>,
    SocketAddr,
    PortGuard,
) {
    let (sender, recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let (_guard, address) = next_addr();
    let cx = SourceContext::new_test(sender, None);
    tokio::spawn(async move {
        SplunkConfig {
            address,
            token: Some(TOKEN.to_owned().into()),
            valid_tokens: None,
            tls: None,
            acknowledgements: Default::default(),
            store_hec_token: false,
            log_namespace: None,
            keepalive: Default::default(),
            event,
            raw,
        }
        .build(cx)
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;
    (recv, address, _guard)
}

/// Codec config that just sets `decoding` (default framing).
fn codec_decoding(decoding: DeserializerConfig) -> CodecConfig {
    CodecConfig {
        framing: None,
        decoding: Some(decoding),
    }
}

/// Codec config that sets both `framing` and `decoding`.
fn codec_full(framing: Option<FramingConfig>, decoding: Option<DeserializerConfig>) -> CodecConfig {
    CodecConfig { framing, decoding }
}

#[tokio::test]
async fn decoder_event_endpoint_json_string() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            CodecConfig::default(),
        )
        .await;
        let envelope =
            r#"{"event":"{\"foo\":\"bar\",\"n\":42}","host":"client-host","sourcetype":"my-app"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );

        let event = collect_n(source, 1).await.remove(0);
        let log = event.as_log();
        assert_eq!(log["foo"], "bar".into());
        assert_eq!(log["n"], 42.into());
        assert_eq!(
            log[log_schema().host_key().unwrap().to_string().as_str()],
            "client-host".into()
        );
        assert_eq!(log[&super::SOURCETYPE], "my-app".into());
    })
    .await;
}

#[tokio::test]
async fn decoder_event_endpoint_json_object_round_trip() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            CodecConfig::default(),
        )
        .await;
        let envelope = r#"{"event":{"foo":"bar","nested":{"k":1}},"host":"h"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );

        let event = collect_n(source, 1).await.remove(0);
        let log = event.as_log();
        assert_eq!(log["foo"], "bar".into());
        assert_eq!(*log.get(event_path!("nested", "k")).unwrap(), 1.into());
        assert_eq!(
            log[log_schema().host_key().unwrap().to_string().as_str()],
            "h".into()
        );
    })
    .await;
}

#[tokio::test]
async fn decoder_event_endpoint_all_envelope_fields_yield_to_decoder() {
    // The decoded path must defer to the codec for `splunk_channel`,
    // `splunk_index`, `splunk_source`, and `splunk_sourcetype` in legacy ns -
    // not just `host`. Otherwise the changelog's "decoder wins on conflict"
    // promise is broken for HEC envelope metadata.
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            CodecConfig::default(),
        )
        .await;
        // The string `event` decodes to a JSON object that pre-populates each
        // legacy splunk_* field. The envelope sets conflicting values for the
        // same fields and must lose.
        let envelope = r#"{
            "event":"{\"splunk_channel\":\"decoder-channel\",\"splunk_index\":\"decoder-index\",\"splunk_source\":\"decoder-source\",\"splunk_sourcetype\":\"decoder-sourcetype\"}",
            "index":"envelope-index",
            "source":"envelope-source",
            "sourcetype":"envelope-sourcetype"
        }"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );

        let event = collect_n(source, 1).await.remove(0);
        let log = event.as_log();
        assert_eq!(log[&super::CHANNEL], "decoder-channel".into());
        assert_eq!(log[&super::INDEX], "decoder-index".into());
        assert_eq!(log[&super::SOURCE], "decoder-source".into());
        assert_eq!(log[&super::SOURCETYPE], "decoder-sourcetype".into());
    })
    .await;
}

#[tokio::test]
async fn decoder_event_endpoint_decoder_field_wins_over_envelope() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            CodecConfig::default(),
        )
        .await;
        // The string `event` decodes to {host: "decoder-host"}; the envelope sets
        // host: "envelope-host". The decoder's value must win.
        let envelope = r#"{"event":"{\"host\":\"decoder-host\"}","host":"envelope-host"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );

        let event = collect_n(source, 1).await.remove(0);
        let log = event.as_log();
        assert_eq!(
            log[log_schema().host_key().unwrap().to_string().as_str()],
            "decoder-host".into()
        );
    })
    .await;
}

#[tokio::test]
async fn decoder_event_endpoint_decode_failure_returns_200() {
    // A malformed inner JSON must not surface as an HTTP error to the Splunk
    // client - decode failures are swallowed by the codec like other Vector
    // sources do.
    let (_source, address, _guard) = source_with_codec(
        codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
        CodecConfig::default(),
    )
    .await;
    let envelope = r#"{"event":"not valid json {","host":"h"}"#;
    assert_eq!(
        200,
        post(address, "services/collector/event", envelope).await
    );
}

#[tokio::test]
async fn decoder_raw_endpoint_newline_delimited() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            CodecConfig::default(),
            codec_full(
                Some(FramingConfig::NewlineDelimited(Default::default())),
                Some(DeserializerConfig::Bytes),
            ),
        )
        .await;
        let body = "line1\nline2\nline3";
        assert_eq!(200, post(address, "services/collector/raw", body).await);

        let events = collect_n(source, 3).await;
        assert_eq!(events.len(), 3);
        let messages: Vec<String> = events
            .iter()
            .map(|e| {
                e.as_log()[log_schema().message_key().unwrap().to_string()]
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert!(messages.contains(&"line1".to_string()));
        assert!(messages.contains(&"line2".to_string()));
        assert!(messages.contains(&"line3".to_string()));

        // All events share the channel from the request header.
        for event in &events {
            assert_eq!(event.as_log()[&super::CHANNEL], "channel".into());
        }
    })
    .await;
}

#[tokio::test]
async fn decoder_event_endpoint_envelope_without_time_has_fallback_timestamp() {
    // Regression: with a decoder set, an envelope that omits `time` must still
    // produce events with a timestamp (the legacy /event path always wrote one).
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            CodecConfig::default(),
        )
        .await;
        let envelope = r#"{"event":"{\"foo\":\"bar\"}"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", envelope).await
        );

        let event = collect_n(source, 1).await.remove(0);
        assert!(
            event.as_log().get_timestamp().is_some(),
            "decoded event from envelope without `time` field is missing a timestamp"
        );
    })
    .await;
}

#[tokio::test]
async fn decoder_independent_per_endpoint_codecs() {
    // /event and /raw can be configured with completely different codecs and
    // each endpoint applies only its own. Here /event uses JSON decoding (so a
    // string `event` field decodes to fields) and /raw uses newline framing
    // with a bytes decoder (so a multi-line body fans out to N events).
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            codec_full(
                Some(FramingConfig::NewlineDelimited(Default::default())),
                Some(DeserializerConfig::Bytes),
            ),
        )
        .await;

        // /event: JSON decoder turns the inner string into structured fields.
        assert_eq!(
            200,
            post(
                address,
                "services/collector/event",
                r#"{"event":"{\"foo\":\"bar\"}"}"#
            )
            .await
        );
        // /raw: newline framing splits the body into three events.
        assert_eq!(
            200,
            post(address, "services/collector/raw", "a\nb\nc").await
        );

        let events = collect_n(source, 4).await;
        assert_eq!(events.len(), 4);

        // The /event request produces one log with `foo=bar`.
        let event_log = events
            .iter()
            .find(|e| e.as_log().contains(event_path!("foo")))
            .expect("expected /event request to produce a log with `foo` set");
        assert_eq!(event_log.as_log()["foo"], "bar".into());

        // The /raw request produces three logs whose messages are the lines.
        let raw_messages: Vec<String> = events
            .iter()
            .filter(|e| !e.as_log().contains(event_path!("foo")))
            .map(|e| {
                e.as_log()[log_schema().message_key().unwrap().to_string()]
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        assert_eq!(raw_messages.len(), 3);
        assert!(raw_messages.contains(&"a".to_string()));
        assert!(raw_messages.contains(&"b".to_string()));
        assert!(raw_messages.contains(&"c".to_string()));
    })
    .await;
}

/// End-to-end test for the second-stage VRL decoder on `/services/collector/event`.
///
/// Validates the core use case from PR #25312: a VRL program decodes the
/// inner `event` payload *and* reads HEC envelope metadata injected before
/// decoding via `%splunk_hec.*` paths.
#[tokio::test]
async fn decoder_vrl_reads_envelope_metadata() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let vrl_source = r#"
            # Read envelope metadata injected before this VRL program runs.
            .envelope_host = string!(%splunk_hec.host)
            .envelope_sourcetype = string!(%splunk_hec.sourcetype)

            # Decode the inner JSON payload (the bytes of the `event` string).
            . = merge!(parse_json!(string!(.message)), .)
        "#;

        let event_codec = codec_decoding(
            DeserializerConfig::Vrl(VrlDeserializerConfig {
                vrl: VrlDeserializerOptions {
                    source: vrl_source.into(),
                    timezone: None,
                },
            }),
        );

        let (source, address, _guard) =
            source_with_codec(event_codec, CodecConfig::default()).await;

        // Send a HEC event whose `event` field is a JSON-encoded string.
        // The VRL decoder should parse it and also read the envelope host/sourcetype.
        let payload = r#"{"event":"{\"level\":\"info\",\"msg\":\"hello\"}","host":"splunk-host","sourcetype":"my-app"}"#;
        assert_eq!(
            200,
            post(address, "services/collector/event", payload).await
        );

        let event = collect_n(source, 1).await.remove(0);
        let log = event.as_log();

        // Inner JSON decoded correctly.
        assert_eq!(log["level"], "info".into());
        assert_eq!(log["msg"], "hello".into());

        // VRL read envelope metadata via %splunk_hec.* and wrote it to event fields.
        assert_eq!(log["envelope_host"], "splunk-host".into());
        assert_eq!(log["envelope_sourcetype"], "my-app".into());

        // Post-decode splunk_hec metadata still applied (host, sourcetype).
        assert_eq!(
            log[log_schema().host_key().unwrap().to_string().as_str()],
            "splunk-host".into()
        );
        assert_eq!(log[&super::SOURCETYPE], "my-app".into());
    })
    .await;
}

#[tokio::test]
async fn decoder_raw_endpoint_event_has_fallback_timestamp() {
    // Regression: decoded /raw events must carry an ingest timestamp like the
    // legacy raw_event path did via `insert_standard_vector_source_metadata`.
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (source, address, _guard) = source_with_codec(
            CodecConfig::default(),
            codec_full(None, Some(DeserializerConfig::Bytes)),
        )
        .await;
        let body = "hello";
        assert_eq!(200, post(address, "services/collector/raw", body).await);

        let event = collect_n(source, 1).await.remove(0);
        assert!(
            event.as_log().get_timestamp().is_some(),
            "decoded /raw event is missing a timestamp"
        );
    })
    .await;
}

#[tokio::test]
async fn decoder_raw_endpoint_empty_decode_does_not_ack() {
    // Regression: when the decoder produces zero events from a raw payload and
    // acknowledgements are enabled, the response must not include an `ackId`
    // because /services/collector/ack would otherwise report success for data
    // Vector silently dropped.
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (sender, _recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let (_guard, address) = next_addr();
    let cx = SourceContext::new_test(sender, None);
    tokio::spawn(async move {
        SplunkConfig {
            address,
            token: Some(TOKEN.to_owned().into()),
            valid_tokens: None,
            tls: None,
            acknowledgements: ack_config,
            store_hec_token: false,
            log_namespace: None,
            keepalive: Default::default(),
            event: CodecConfig::default(),
            raw: codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
        }
        .build(cx)
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;

    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    // A body the JSON decoder cannot parse - codec drops it, no events emitted.
    let body = "not json {";
    let response = send_with_response(address, "services/collector/raw", body, TOKEN, &opts)
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(response["code"].as_u64(), Some(0), "response: {response:?}");
    assert!(
        response.get("ackId").is_none(),
        "expected no ackId in response when decoder produced zero events, got: {response:?}"
    );
}

#[tokio::test]
async fn decoder_raw_endpoint_partial_decode_does_not_ack() {
    // Regression: a request whose body decodes into some valid frames AND some
    // dropped frames (e.g., `valid \n invalid \n valid` under newline framing
    // with a JSON decoder) must not return an `ackId`. Otherwise
    // /services/collector/ack reports success for data Vector silently dropped.
    let ack_config = HecAcknowledgementsConfig {
        enabled: Some(true),
        ..Default::default()
    };
    let (sender, _recv) = SourceSender::new_test_finalize(EventStatus::Delivered);
    let (_guard, address) = next_addr();
    let cx = SourceContext::new_test(sender, None);
    tokio::spawn(async move {
        SplunkConfig {
            address,
            token: Some(TOKEN.to_owned().into()),
            valid_tokens: None,
            tls: None,
            acknowledgements: ack_config,
            store_hec_token: false,
            log_namespace: None,
            keepalive: Default::default(),
            event: CodecConfig::default(),
            raw: codec_full(
                Some(FramingConfig::NewlineDelimited(Default::default())),
                Some(vector_lib::codecs::JsonDeserializerConfig::default().into()),
            ),
        }
        .build(cx)
        .await
        .unwrap()
        .await
        .unwrap()
    });
    wait_for_tcp(address).await;

    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    // Two valid JSON frames bracketing one invalid frame.
    let body = "{\"valid\":1}\nnot json\n{\"valid\":2}";
    let response = send_with_response(address, "services/collector/raw", body, TOKEN, &opts)
        .await
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(response["code"].as_u64(), Some(0), "response: {response:?}");
    assert!(
        response.get("ackId").is_none(),
        "expected no ackId when the decoder dropped a frame mid-request, got: {response:?}"
    );
}

#[tokio::test]
async fn decoder_event_endpoint_error_index_matches_envelope_not_fanout() {
    // Regression: with the decoder fanning out one envelope into many events,
    // `InvalidEventNumber` in error responses must still report the failing
    // envelope's zero-indexed position, not the cumulative event count.
    let (source, address, _guard) = source_with_codec(
        codec_full(
            Some(FramingConfig::NewlineDelimited(Default::default())),
            Some(DeserializerConfig::Bytes),
        ),
        CodecConfig::default(),
    )
    .await;
    // Envelope 0 has an `event` string with three lines: with newline framing
    // and a bytes decoder, that fans out to three events. Envelope 1 omits
    // `event`, so the decoded path returns `MissingEventField { event: 1 }`.
    let body = "{\"event\":\"a\\nb\\nc\"}{\"foo\":\"bar\"}";

    let opts = SendWithOpts {
        channel: Some(Channel::Header("guid")),
        forwarded_for: None,
    };
    let response =
        send_with_response(address, "services/collector/event", body, TOKEN, &opts).await;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap();

    assert_eq!(status.as_u16(), 400, "body: {body:?}");
    assert_eq!(
        body["invalid-event-number"].as_u64(),
        Some(1),
        "expected envelope index 1 (the failing envelope), not a fan-out event index. body: {body:?}"
    );
    // Drain the partially-emitted events so the source task doesn't block.
    let _ = collect_n(source, 3).await;
}

#[test]
fn output_schema_definition_with_decoder_vector_namespace() {
    let config = SplunkConfig {
        log_namespace: Some(true),
        event: codec_decoding(vector_lib::codecs::JsonDeserializerConfig::default().into()),
        ..Default::default()
    };
    let definition = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    // The decoder's schema produces `Kind::json()` at the root, the source
    // layers its envelope metadata fields on top, and the legacy log shape is
    // unioned in (since /raw has no decoder and still emits legacy events) -
    // contributing the `message` meaning at root.
    let expected_definition =
        Definition::new_with_default_metadata(Kind::json(), [LogNamespace::Vector])
            .with_meaning(OwnedTargetPath::event_root(), meaning::MESSAGE)
            .with_metadata_field(
                &owned_value_path!("vector", "source_type"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("vector", "ingest_timestamp"),
                Kind::timestamp(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("splunk_hec", "host"),
                Kind::bytes(),
                Some("host"),
            )
            .with_metadata_field(
                &owned_value_path!("splunk_hec", "index"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("splunk_hec", "source"),
                Kind::bytes(),
                Some("service"),
            )
            .with_metadata_field(
                &owned_value_path!("splunk_hec", "channel"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("splunk_hec", "sourcetype"),
                Kind::bytes(),
                None,
            );

    assert_eq!(definition, Some(expected_definition));
}

#[test]
fn output_schema_definition_vector_namespace() {
    let config = SplunkConfig {
        log_namespace: Some(true),
        ..Default::default()
    };

    let definition = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()).or_bytes(),
        [LogNamespace::Vector],
    )
    .with_meaning(OwnedTargetPath::event_root(), meaning::MESSAGE)
    .with_metadata_field(
        &owned_value_path!("vector", "source_type"),
        Kind::bytes(),
        None,
    )
    .with_metadata_field(
        &owned_value_path!("vector", "ingest_timestamp"),
        Kind::timestamp(),
        None,
    )
    .with_metadata_field(
        &owned_value_path!("splunk_hec", "host"),
        Kind::bytes(),
        Some("host"),
    )
    .with_metadata_field(
        &owned_value_path!("splunk_hec", "index"),
        Kind::bytes(),
        None,
    )
    .with_metadata_field(
        &owned_value_path!("splunk_hec", "source"),
        Kind::bytes(),
        Some("service"),
    )
    .with_metadata_field(
        &owned_value_path!("splunk_hec", "channel"),
        Kind::bytes(),
        None,
    )
    .with_metadata_field(
        &owned_value_path!("splunk_hec", "sourcetype"),
        Kind::bytes(),
        None,
    );

    assert_eq!(definition, Some(expected_definition));
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let config = SplunkConfig::default();
    let definitions = config
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()),
        [LogNamespace::Legacy],
    )
    .with_event_field(&owned_value_path!("host"), Kind::bytes(), Some("host"))
    .with_event_field(
        &owned_value_path!("message"),
        Kind::bytes().or_undefined(),
        Some("message"),
    )
    .with_event_field(
        &owned_value_path!("line"),
        Kind::array(Collection::empty())
            .or_object(Collection::empty())
            .or_undefined(),
        None,
    )
    .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("splunk_channel"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("splunk_index"), Kind::bytes(), None)
    .with_event_field(
        &owned_value_path!("splunk_source"),
        Kind::bytes(),
        Some("service"),
    )
    .with_event_field(&owned_value_path!("splunk_sourcetype"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None);

    assert_eq!(definitions, Some(expected_definition));
}

impl ValidatableComponent for SplunkConfig {
    fn validation_configuration() -> ValidationConfiguration {
        let config = Self {
            address: default_socket_address(),
            ..Default::default()
        };

        let listen_addr_http = format!("http://{}/services/collector/event", config.address);
        let uri = Uri::try_from(&listen_addr_http).expect("should not fail to parse URI");

        let log_namespace: LogNamespace = config.log_namespace.unwrap_or_default().into();
        let framing = BytesDecoderConfig::new().into();
        let decoding = DeserializerConfig::Json(Default::default());

        let external_resource = ExternalResource::new(
            ResourceDirection::Push,
            HttpResourceConfig::from_parts(uri, None).with_headers(HashMap::from([(
                X_SPLUNK_REQUEST_CHANNEL.to_string(),
                "channel".to_string(),
            )])),
            DecodingConfig::new(framing, decoding, false.into()),
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

register_validatable_component!(SplunkConfig);
