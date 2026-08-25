use std::{io::Write, net::SocketAddr, str::FromStr};

use flate2::{
    Compression,
    write::{GzEncoder, ZlibEncoder},
};
use futures::Stream;
use headers::{Authorization, authorization::Credentials};
use http::{HeaderMap, Method, StatusCode, Uri, header::AUTHORIZATION};
use similar_asserts::assert_eq;
use vector_lib::{
    codecs::{
        BytesDecoderConfig, JsonDeserializerConfig,
        decoding::{DeserializerConfig, FramingConfig},
    },
    config::LogNamespace,
    event::LogEvent,
    lookup::{
        OwnedTargetPath, PathPrefix, event_path, lookup_v2::OptionalValuePath, owned_value_path,
        path,
    },
    schema::Definition,
};
use vrl::{
    path::ValuePath as _,
    value::{Kind, ObjectMap, kind::Collection},
};

use super::{SimpleHttpConfig, remove_duplicates};
use crate::{
    SourceSender,
    common::http::server_auth::HttpServerAuthConfig,
    components::validation::prelude::*,
    config::{SourceConfig, SourceContext, log_schema},
    event::{Event, EventStatus, Value},
    sources::http_server::HttpMethod,
    test_util::{
        addr::next_addr,
        components::{self, HTTP_PUSH_SOURCE_TAGS, assert_source_compliance},
        spawn_collect_n, wait_for_tcp,
    },
};

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<SimpleHttpConfig>();
}

#[allow(clippy::too_many_arguments)]
async fn source<'a>(
    headers: Vec<String>,
    query_parameters: Vec<String>,
    path_key: &'a str,
    host_key: &'a str,
    path: &'a str,
    method: &'a str,
    response_code: StatusCode,
    auth: Option<HttpServerAuthConfig>,
    strict_path: bool,
    status: EventStatus,
    acknowledgements: bool,
    framing: Option<FramingConfig>,
    decoding: Option<DeserializerConfig>,
) -> (impl Stream<Item = Event> + 'a, SocketAddr) {
    let (sender, recv) = SourceSender::new_test_finalize(status);
    let (_guard, address) = next_addr();
    let path = path.to_owned();
    let host_key = OptionalValuePath::from(owned_value_path!(host_key));
    let path_key = OptionalValuePath::from(owned_value_path!(path_key));
    let context = SourceContext::new_test(sender, None);
    let method = match Method::from_str(method).unwrap() {
        Method::GET => HttpMethod::Get,
        Method::POST => HttpMethod::Post,
        _ => HttpMethod::Post,
    };

    tokio::spawn(async move {
        SimpleHttpConfig {
            address,
            headers,
            query_parameters,
            response_code,
            tls: None,
            auth,
            strict_path,
            path_key,
            host_key,
            path,
            method,
            framing,
            decoding,
            acknowledgements: acknowledgements.into(),
            log_namespace: None,
            keepalive: Default::default(),
        }
        .build(context)
        .await
        .unwrap()
        .await
        .unwrap();
    });
    wait_for_tcp(address).await;
    (recv, address)
}

async fn send(address: SocketAddr, body: &str) -> u16 {
    reqwest::Client::new()
        .post(format!("http://{address}/"))
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn send_with_headers(address: SocketAddr, body: &str, headers: HeaderMap) -> u16 {
    reqwest::Client::new()
        .post(format!("http://{address}/"))
        .headers(headers)
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn send_with_query(address: SocketAddr, body: &str, query: &str) -> u16 {
    reqwest::Client::new()
        .post(format!("http://{address}?{query}"))
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn send_with_path(address: SocketAddr, body: &str, path: &str) -> u16 {
    reqwest::Client::new()
        .post(format!("http://{address}{path}"))
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn send_request(address: SocketAddr, method: &str, body: &str, path: &str) -> u16 {
    let method = Method::from_bytes(method.to_owned().as_bytes()).unwrap();
    reqwest::Client::new()
        .request(method, format!("http://{address}{path}"))
        .body(body.to_owned())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn send_bytes(address: SocketAddr, body: Vec<u8>, headers: HeaderMap) -> u16 {
    reqwest::Client::new()
        .post(format!("http://{address}/"))
        .headers(headers)
        .body(body)
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

async fn spawn_ok_collect_n(
    send: impl std::future::Future<Output = u16> + Send + 'static,
    rx: impl Stream<Item = Event> + Unpin,
    n: usize,
) -> Vec<Event> {
    spawn_collect_n(async move { assert_eq!(200, send.await) }, rx, n).await
}

#[tokio::test]
async fn http_multiline_text() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
        let body = "test body\ntest body 2";

        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        spawn_ok_collect_n(send(addr, body), rx, 2).await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "test body".into());
        assert!(log.get_timestamp().is_some());
        assert_eq!(
            *log.get_source_type().unwrap(),
            SimpleHttpConfig::NAME.into()
        );
        assert_eq!(log["http_path"], "/".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "test body 2".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_multiline_text2() {
    //same as above test but with a newline at the end
    let body = "test body\ntest body 2\n";

    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        spawn_ok_collect_n(send(addr, body), rx, 2).await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "test body".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "test body 2".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_bytes_codec_preserves_newlines() {
    let body = "foo\nbar";

    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            Some(BytesDecoderConfig::new().into()),
            None,
        )
        .await;

        spawn_ok_collect_n(send(addr, body), rx, 1).await
    })
    .await;

    assert_eq!(events.len(), 1);

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "foo\nbar".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_json_parsing() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(400, send(addr, "{").await); //malformed
                assert_eq!(400, send(addr, r#"{"key"}"#).await); //key without value

                assert_eq!(200, send(addr, "{}").await); //can be one object or array of objects
                assert_eq!(200, send(addr, "[{},{},{}]").await);
            },
            rx,
            2,
        )
        .await
    })
    .await;

    assert!(events.remove(1).as_log().get_timestamp().is_some());
    assert!(events.remove(0).as_log().get_timestamp().is_some());
}

#[tokio::test]
async fn http_json_values() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(200, send(addr, r#"[{"key":"value"}]"#).await);
                assert_eq!(200, send(addr, r#"{"key2":"value2"}"#).await);
            },
            rx,
            2,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key"], "value".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key2"], "value2".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_json_dotted_keys() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(200, send(addr, r#"[{"dotted.key":"value"}]"#).await);
                assert_eq!(
                    200,
                    send(addr, r#"{"nested":{"dotted.key2":"value2"}}"#).await
                );
            },
            rx,
            2,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(
            log.get(event_path!("dotted.key")).unwrap(),
            &Value::from("value")
        );
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        let mut map = ObjectMap::new();
        map.insert("dotted.key2".into(), Value::from("value2"));
        assert_eq!(log["nested"], map.into());
    }
}

#[tokio::test]
async fn http_ndjson() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send(addr, r#"[{"key1":"value1"},{"key2":"value2"}]"#).await
                );

                assert_eq!(
                    200,
                    send(addr, "{\"key1\":\"value1\"}\n\n{\"key2\":\"value2\"}").await
                );
            },
            rx,
            4,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key2"], "value2".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_event_metadata(log).await;
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key2"], "value2".into());
        assert_event_metadata(log).await;
    }
}

async fn assert_event_metadata(log: &LogEvent) {
    assert!(log.get_timestamp().is_some());

    let source_type_key_value = log
        .get((PathPrefix::Event, log_schema().source_type_key().unwrap()))
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(source_type_key_value, SimpleHttpConfig::NAME);
    assert_eq!(log["http_path"], "/".into());
}

#[tokio::test]
async fn http_headers() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "test_client".parse().unwrap());
        headers.insert("Upgrade-Insecure-Requests", "false".parse().unwrap());
        headers.insert("X-Test-Header", "true".parse().unwrap());

        let (rx, addr) = source(
            vec![
                "User-Agent".to_string(),
                "Upgrade-Insecure-Requests".to_string(),
                "X-*".to_string(),
                "AbsentHeader".to_string(),
            ],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_ok_collect_n(
            send_with_headers(addr, "{\"key1\":\"value1\"}", headers),
            rx,
            1,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_eq!(log["\"User-Agent\""], "test_client".into());
        assert_eq!(log["\"Upgrade-Insecure-Requests\""], "false".into());
        assert_eq!(log["\"x-test-header\""], "true".into());
        assert_eq!(log["AbsentHeader"], Value::Null);
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_headers_wildcard() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let mut headers = HeaderMap::new();
        headers.insert("User-Agent", "test_client".parse().unwrap());
        headers.insert("X-Case-Sensitive-Value", "CaseSensitive".parse().unwrap());
        // Header that conflicts with an existing field.
        headers.insert("key1", "value_from_header".parse().unwrap());

        let (rx, addr) = source(
            vec!["*".to_string()],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_ok_collect_n(
            send_with_headers(addr, "{\"key1\":\"value1\"}", headers),
            rx,
            1,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_eq!(log["\"user-agent\""], "test_client".into());
        assert_eq!(log["\"x-case-sensitive-value\""], "CaseSensitive".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_query() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![
                "source".to_string(),
                "region".to_string(),
                "absent".to_string(),
            ],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_ok_collect_n(
            send_with_query(addr, "{\"key1\":\"value1\"}", "source=staging&region=gb"),
            rx,
            1,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_eq!(log["source"], "staging".into());
        assert_eq!(log["region"], "gb".into());
        assert_eq!(log["absent"], Value::Null);
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_query_wildcard() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec!["*".to_string()],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_ok_collect_n(
            send_with_query(
                addr,
                "{\"key1\":\"value1\",\"key2\":\"value2\"}",
                "source=staging&region=gb&key1=value_from_query",
            ),
            rx,
            1,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value_from_query".into());
        assert_eq!(log["key2"], "value2".into());
        assert_eq!(log["source"], "staging".into());
        assert_eq!(log["region"], "gb".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_gzip_deflate() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let body = "test body";

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_bytes()).unwrap();
        let body = encoder.finish().unwrap();

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(body.as_slice()).unwrap();
        let body = encoder.finish().unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("Content-Encoding", "gzip, deflate".parse().unwrap());

        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        spawn_ok_collect_n(send_bytes(addr, body, headers), rx, 1).await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(*log.get_message().unwrap(), "test body".into());
        assert_event_metadata(log).await;
    }
}

#[tokio::test]
async fn http_rejects_gzip_bomb_with_413() {
    // A modestly-sized gzipped blob of zeros that would expand past the default
    // 100 MiB cap if decompression were unbounded.
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let chunk = [0u8; 8 * 1024];
    for _ in 0..(200 * 1024 * 1024 / chunk.len()) {
        encoder.write_all(&chunk).unwrap();
    }
    let body = encoder.finish().unwrap();

    let mut headers = HeaderMap::new();
    headers.insert("Content-Encoding", "gzip".parse().unwrap());

    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "http_path",
        "remote_ip",
        "/",
        "POST",
        StatusCode::OK,
        None,
        true,
        EventStatus::Delivered,
        true,
        None,
        None,
    )
    .await;

    assert_eq!(413, send_bytes(addr, body, headers).await);
}

#[tokio::test]
async fn http_path() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "vector_http_path",
            "vector_remote_ip",
            "/event/path",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_ok_collect_n(
            send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path"),
            rx,
            1,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_eq!(log["vector_http_path"], "/event/path".into());
        assert!(log.get_timestamp().is_some());
        assert_eq!(
            *log.get_source_type().unwrap(),
            SimpleHttpConfig::NAME.into()
        );
    }
}

#[tokio::test]
async fn http_path_no_restriction() {
    let mut events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "vector_http_path",
            "vector_remote_ip",
            "/event",
            "POST",
            StatusCode::OK,
            None,
            false,
            EventStatus::Delivered,
            true,
            None,
            Some(JsonDeserializerConfig::default().into()),
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(
                    200,
                    send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path1").await
                );
                assert_eq!(
                    200,
                    send_with_path(addr, "{\"key2\":\"value2\"}", "/event/path2").await
                );
            },
            rx,
            2,
        )
        .await
    })
    .await;

    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key1"], "value1".into());
        assert_eq!(log["vector_http_path"], "/event/path1".into());
        assert!(log.get_timestamp().is_some());
        assert_eq!(
            *log.get_source_type().unwrap(),
            SimpleHttpConfig::NAME.into()
        );
    }
    {
        let event = events.remove(0);
        let log = event.as_log();
        assert_eq!(log["key2"], "value2".into());
        assert_eq!(log["vector_http_path"], "/event/path2".into());
        assert!(log.get_timestamp().is_some());
        assert_eq!(
            *log.get_source_type().unwrap(),
            SimpleHttpConfig::NAME.into()
        );
    }
}

#[tokio::test]
async fn http_wrong_path() {
    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "vector_http_path",
        "vector_remote_ip",
        "/",
        "POST",
        StatusCode::OK,
        None,
        true,
        EventStatus::Delivered,
        true,
        None,
        Some(JsonDeserializerConfig::default().into()),
    )
    .await;

    assert_eq!(
        404,
        send_with_path(addr, "{\"key1\":\"value1\"}", "/event/path").await
    );
}

#[tokio::test]
async fn http_status_code() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async move {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::ACCEPTED,
            None,
            true,
            EventStatus::Delivered,
            true,
            None,
            None,
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(
                    StatusCode::ACCEPTED,
                    send(addr, "{\"key1\":\"value1\"}").await
                );
            },
            rx,
            1,
        )
        .await;
    })
    .await;
}

#[tokio::test]
async fn http_delivery_failure() {
    assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Rejected,
            true,
            None,
            None,
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(400, send(addr, "test body\n").await);
            },
            rx,
            1,
        )
        .await;
    })
    .await;
}

#[tokio::test]
async fn ignores_disabled_acknowledgements() {
    let events = assert_source_compliance(&HTTP_PUSH_SOURCE_TAGS, async {
        let (rx, addr) = source(
            vec![],
            vec![],
            "http_path",
            "remote_ip",
            "/",
            "POST",
            StatusCode::OK,
            None,
            true,
            EventStatus::Rejected,
            false,
            None,
            None,
        )
        .await;

        spawn_collect_n(
            async move {
                assert_eq!(200, send(addr, "test body\n").await);
            },
            rx,
            1,
        )
        .await
    })
    .await;

    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn http_get_method() {
    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "http_path",
        "remote_ip",
        "/",
        "GET",
        StatusCode::OK,
        None,
        true,
        EventStatus::Delivered,
        true,
        None,
        None,
    )
    .await;

    assert_eq!(200, send_request(addr, "GET", "", "/").await);
}

#[tokio::test]
async fn returns_401_when_required_auth_is_missing() {
    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "http_path",
        "remote_ip",
        "/",
        "GET",
        StatusCode::OK,
        Some(HttpServerAuthConfig::Basic {
            username: "test".to_string(),
            password: "test".to_string().into(),
        }),
        true,
        EventStatus::Delivered,
        true,
        None,
        None,
    )
    .await;

    assert_eq!(401, send_request(addr, "GET", "", "/").await);
}

#[tokio::test]
async fn returns_401_when_required_auth_is_wrong() {
    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "http_path",
        "remote_ip",
        "/",
        "POST",
        StatusCode::OK,
        Some(HttpServerAuthConfig::Basic {
            username: "test".to_string(),
            password: "test".to_string().into(),
        }),
        true,
        EventStatus::Delivered,
        true,
        None,
        None,
    )
    .await;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        Authorization::basic("wrong", "test").0.encode(),
    );
    assert_eq!(401, send_with_headers(addr, "", headers).await);
}

#[tokio::test]
async fn http_get_with_correct_auth() {
    components::init_test();
    let (_rx, addr) = source(
        vec![],
        vec![],
        "http_path",
        "remote_ip",
        "/",
        "POST",
        StatusCode::OK,
        Some(HttpServerAuthConfig::Basic {
            username: "test".to_string(),
            password: "test".to_string().into(),
        }),
        true,
        EventStatus::Delivered,
        true,
        None,
        None,
    )
    .await;

    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        Authorization::basic("test", "test").0.encode(),
    );
    assert_eq!(200, send_with_headers(addr, "", headers).await);
}

#[test]
fn output_schema_definition_vector_namespace() {
    let config = SimpleHttpConfig {
        log_namespace: Some(true),
        ..Default::default()
    };

    let definitions = config
        .outputs(LogNamespace::Vector)
        .remove(0)
        .schema_definition(true);

    let expected_definition =
        Definition::new_with_default_metadata(Kind::bytes(), [LogNamespace::Vector])
            .with_meaning(OwnedTargetPath::event_root(), "message")
            .with_metadata_field(
                &owned_value_path!("vector", "source_type"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(SimpleHttpConfig::NAME, "path"),
                Kind::bytes(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(SimpleHttpConfig::NAME, "headers"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(SimpleHttpConfig::NAME, "query_parameters"),
                Kind::object(Collection::empty().with_unknown(Kind::bytes())).or_undefined(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!(SimpleHttpConfig::NAME, "host"),
                Kind::bytes().or_undefined(),
                None,
            )
            .with_metadata_field(
                &owned_value_path!("vector", "ingest_timestamp"),
                Kind::timestamp(),
                None,
            );

    assert_eq!(definitions, Some(expected_definition))
}

#[test]
fn output_schema_definition_legacy_namespace() {
    let config = SimpleHttpConfig::default();

    let definitions = config
        .outputs(LogNamespace::Legacy)
        .remove(0)
        .schema_definition(true);

    let expected_definition = Definition::new_with_default_metadata(
        Kind::object(Collection::empty()),
        [LogNamespace::Legacy],
    )
    .with_event_field(
        &owned_value_path!("message"),
        Kind::bytes(),
        Some("message"),
    )
    .with_event_field(&owned_value_path!("source_type"), Kind::bytes(), None)
    .with_event_field(&owned_value_path!("timestamp"), Kind::timestamp(), None)
    .with_event_field(&owned_value_path!("path"), Kind::bytes(), None)
    .with_event_field(
        &owned_value_path!("host"),
        Kind::bytes().or_undefined(),
        None,
    )
    .unknown_fields(Kind::bytes());

    assert_eq!(definitions, Some(expected_definition))
}

#[test]
fn validate_remove_duplicates() {
    let mut list = vec![
        "a".to_owned(),
        "b".to_owned(),
        "c".to_owned(),
        "d".to_owned(),
    ];

    // no duplicates should be identical
    {
        let list_dedup = remove_duplicates(list.clone(), "foo");

        assert_eq!(list, list_dedup);
    }

    list.push("b".to_owned());

    // remove duplicate "b"
    {
        let list_dedup = remove_duplicates(list.clone(), "foo");
        assert_eq!(
            vec![
                "a".to_owned(),
                "b".to_owned(),
                "c".to_owned(),
                "d".to_owned()
            ],
            list_dedup
        );
    }
}

#[test]
fn inject_auth_enrichment_does_not_clobber_vector_namespace_builtin_fields() {
    use crate::{codecs::DecodingConfig, sources::util::HttpSource as _};
    use vector_lib::codecs::BytesDeserializerConfig;
    use vrl::value::KeyString;

    let decoder = DecodingConfig::new(
        BytesDecoderConfig::new().into(),
        BytesDeserializerConfig::new().into(),
        LogNamespace::Vector,
    )
    .build()
    .unwrap()
    .with_log_namespace(LogNamespace::Vector);

    let source = super::SimpleHttpSource {
        headers: vec![],
        query_parameters: vec![],
        path_key: OptionalValuePath::none(),
        host_key: OptionalValuePath::none(),
        decoder,
        log_namespace: LogNamespace::Vector,
    };

    let mut log = LogEvent::default();
    // Pre-populate %http_server.path as enrich_events would.
    log.insert(
        (
            PathPrefix::Metadata,
            path!(SimpleHttpConfig::NAME).concat(path!("path")),
        ),
        "/real/path",
    );

    let mut events = vec![Event::Log(log)];
    let mut enrichment = ObjectMap::new();
    // Attempt to clobber the built-in `path` field and inject a new field.
    enrichment.insert(KeyString::from("path"), Value::from("/clobbered"));
    enrichment.insert(KeyString::from("tenant_id"), Value::from("t-123"));

    source.inject_auth_enrichment(&mut events, enrichment);

    let Event::Log(log) = &events[0] else {
        panic!("expected log event");
    };
    assert_eq!(
        log.get((
            PathPrefix::Metadata,
            path!(SimpleHttpConfig::NAME).concat(path!("path")),
        )),
        Some(&Value::from("/real/path")),
        "auth enrichment must not overwrite built-in source metadata"
    );
    assert_eq!(
        log.get((
            PathPrefix::Metadata,
            path!(SimpleHttpConfig::NAME).concat(path!("tenant_id")),
        )),
        Some(&Value::from("t-123")),
        "new auth enrichment field must be injected"
    );
}

#[test]
fn inject_auth_enrichment_does_not_overwrite_existing_metadata_in_vector_namespace() {
    use crate::{codecs::DecodingConfig, sources::util::HttpSource as _};
    use vector_lib::codecs::BytesDeserializerConfig;
    use vrl::value::KeyString;

    let decoder = DecodingConfig::new(
        BytesDecoderConfig::new().into(),
        BytesDeserializerConfig::new().into(),
        LogNamespace::Vector,
    )
    .build()
    .unwrap()
    .with_log_namespace(LogNamespace::Vector);

    let source = super::SimpleHttpSource {
        headers: vec![],
        query_parameters: vec![],
        path_key: OptionalValuePath::none(),
        host_key: OptionalValuePath::none(),
        decoder,
        log_namespace: LogNamespace::Vector,
    };

    let mut log = LogEvent::default();
    // Pre-populate a key (e.g. already written by enrich_events or the decoded event).
    log.insert(
        (
            PathPrefix::Metadata,
            path!(SimpleHttpConfig::NAME).concat(path!("tenant_id")),
        ),
        "existing",
    );

    let mut events = vec![Event::Log(log)];
    let mut enrichment = ObjectMap::new();
    enrichment.insert(KeyString::from("tenant_id"), Value::from("auth-value"));

    source.inject_auth_enrichment(&mut events, enrichment);

    let Event::Log(log) = &events[0] else {
        panic!("expected log event");
    };
    assert_eq!(
        log.get((
            PathPrefix::Metadata,
            path!(SimpleHttpConfig::NAME).concat(path!("tenant_id")),
        )),
        Some(&Value::from("existing")),
        "auth enrichment must not overwrite already-present metadata keys"
    );
}

#[test]
fn inject_auth_enrichment_applies_to_non_log_events_in_vector_namespace() {
    use crate::{codecs::DecodingConfig, sources::util::HttpSource as _};
    use vector_lib::{
        codecs::BytesDeserializerConfig,
        event::{Metric, MetricKind, MetricValue},
    };
    use vrl::value::KeyString;

    let decoder = DecodingConfig::new(
        BytesDecoderConfig::new().into(),
        BytesDeserializerConfig::new().into(),
        LogNamespace::Vector,
    )
    .build()
    .unwrap()
    .with_log_namespace(LogNamespace::Vector);

    let source = super::SimpleHttpSource {
        headers: vec![],
        query_parameters: vec![],
        path_key: OptionalValuePath::none(),
        host_key: OptionalValuePath::none(),
        decoder,
        log_namespace: LogNamespace::Vector,
    };

    let metric = Metric::new(
        "requests",
        MetricKind::Incremental,
        MetricValue::Counter { value: 1.0 },
    );
    let mut events = vec![Event::Metric(metric)];

    let mut enrichment = ObjectMap::new();
    enrichment.insert(KeyString::from("tenant_id"), Value::from("t-456"));

    source.inject_auth_enrichment(&mut events, enrichment);

    let Event::Metric(metric) = &events[0] else {
        panic!("expected metric event");
    };
    assert_eq!(
        metric
            .metadata()
            .value()
            .get(path!(SimpleHttpConfig::NAME).concat(path!("tenant_id")),),
        Some(&Value::from("t-456")),
        "auth enrichment must be written to non-log event metadata"
    );
}

impl ValidatableComponent for SimpleHttpConfig {
    fn validation_configuration() -> ValidationConfiguration {
        let config = Self {
            decoding: Some(DeserializerConfig::Json(Default::default())),
            ..Default::default()
        };

        let log_namespace: LogNamespace = config.log_namespace.unwrap_or(false).into();

        let listen_addr_http = format!("http://{}/", config.address);
        let uri = Uri::try_from(&listen_addr_http).expect("should not fail to parse URI");

        let external_resource = ExternalResource::new(
            ResourceDirection::Push,
            HttpResourceConfig::from_parts(uri, Some(config.method.into())),
            config
                .get_decoding_config()
                .expect("should not fail to get decoding config"),
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

register_validatable_component!(SimpleHttpConfig);
