use std::{
    ffi::OsStr,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use bytes::Bytes;
use futures::StreamExt as _;
use http::{Response, StatusCode, request::Parts};
use http_body_util::Full;
use opendal::ErrorKind;
use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    partition::Partitioner,
    request_metadata::GroupedCountByteSize,
};
use vrl::event_path;

use super::config::WebHdfsConfig;
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    config::{SinkConfig, SinkContext},
    event::LogEvent,
    sinks::{
        opendal_common::{OpenDalRequest, OpenDalRequestBuilder, OpenDalRetryLogic},
        util::{
            Compression, HttpEndpoint, TowerRequestConfig,
            request_builder::{EncodeResult, RequestBuilder},
            retries::RetryLogic as _,
            test::build_test_server_generic,
        },
    },
    test_util::addr::next_addr,
};

fn default_config(encoding: EncodingConfigWithFraming) -> WebHdfsConfig {
    WebHdfsConfig {
        root: "/tmp/".to_string(),
        prefix: "%F/".to_string(),
        endpoint: HttpEndpoint::parse("http://127.0.0.1:9870").unwrap(),
        encoding,
        compression: Compression::gzip_default(),
        batch: crate::sinks::util::BatchConfig::default(),
        request: TowerRequestConfig::default(),
        acknowledgements: crate::config::AcknowledgementsConfig::default(),
        confinement: crate::template::ConfinementConfig::default(),
    }
}

#[test]
fn webhdfs_generate_config() {
    crate::test_util::test_generate_config::<WebHdfsConfig>();
}

fn request_builder(sink_config: &WebHdfsConfig) -> OpenDalRequestBuilder {
    let transformer = sink_config.encoding.transformer();
    let (framer, serializer) = sink_config
        .encoding
        .build(SinkType::MessageBased)
        .expect("encoding must build with success");
    let encoder = Encoder::<Framer>::new(framer, serializer);

    OpenDalRequestBuilder {
        encoder: (transformer, encoder),
        compression: sink_config.compression,
    }
}

fn build_request(compression: Compression) -> OpenDalRequest {
    let sink_config = WebHdfsConfig {
        compression,
        ..default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
        )
    };
    let log = LogEvent::default().into();
    let key = sink_config
        .key_partitioner()
        .unwrap()
        .partition(&log)
        .expect("key wasn't provided");
    let request_builder = request_builder(&sink_config);
    let (metadata, metadata_request_builder, _events) =
        request_builder.split_input((key, vec![log]));
    let byte_size = GroupedCountByteSize::new_untagged();
    let payload = EncodeResult::uncompressed(Bytes::new(), byte_size);
    let request_metadata = metadata_request_builder.build(&payload);

    request_builder.build_request(metadata, request_metadata, payload)
}

#[test]
fn webhdfs_build_request() {
    let req = build_request(Compression::None);
    assert_eq!(
        Path::new(&req.metadata.partition_key).extension(),
        Some(OsStr::new("log"))
    );

    let req = build_request(Compression::None);
    assert_eq!(
        Path::new(&req.metadata.partition_key).extension(),
        Some(OsStr::new("log"))
    );

    let req = build_request(Compression::gzip_default());
    assert!(req.metadata.partition_key.ends_with(".log.gz"));

    let req = build_request(Compression::zlib_default());
    assert!(req.metadata.partition_key.ends_with(".log.zz"));
}

#[test]
fn webhdfs_retry_logic_retries_only_temporary_errors() {
    let temporary = opendal::Error::new(ErrorKind::Unexpected, "send http request").set_temporary();
    let permanent = opendal::Error::new(ErrorKind::PermissionDenied, "permission denied");

    assert!(OpenDalRetryLogic.is_retriable_error(&temporary));
    assert!(!OpenDalRetryLogic.is_retriable_error(&permanent));
}

/// Runs one event through the sink built from its config, against a fake WebHDFS server. A write is
/// a `CREATE` call answered with a location, then a `PUT` of the payload there; the first `PUT`
/// gets `first_put_status` and later ones succeed. Returns the requests the server accepted and
/// the number of requests it received.
async fn write_through_sink(first_put_status: StatusCode) -> (Vec<(Parts, Bytes)>, usize) {
    let (_guard, addr) = next_addr();
    let calls = Arc::new(AtomicUsize::new(0));
    let server_calls = Arc::clone(&calls);
    let (rx, trigger, server) = build_test_server_generic(addr, move || {
        let (status, body) = match server_calls.fetch_add(1, Ordering::SeqCst) {
            call if call % 2 == 0 => (
                StatusCode::OK,
                format!(r#"{{"Location":"http://{addr}/data"}}"#),
            ),
            1 => (first_put_status, String::new()),
            _ => (StatusCode::CREATED, String::new()),
        };
        Response::builder()
            .status(status)
            .body(Full::new(Bytes::from(body)))
            .unwrap()
    });
    tokio::spawn(server);

    let config = WebHdfsConfig {
        endpoint: HttpEndpoint::parse(&format!("http://{addr}")).unwrap(),
        compression: Compression::None,
        ..default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                JsonSerializerConfig::default(),
            )
                .into(),
        )
    };
    let (sink, _) = SinkConfig::build(&config, SinkContext::default())
        .await
        .unwrap();
    let mut log = LogEvent::default();
    log.insert(event_path!("message"), "hello");
    sink.run_events([log.into()]).await.unwrap();

    drop(trigger);
    (rx.collect().await, calls.load(Ordering::SeqCst))
}

#[tokio::test]
async fn webhdfs_retries_temporary_write_error() {
    let (accepted, calls) = write_through_sink(StatusCode::SERVICE_UNAVAILABLE).await;

    assert_eq!(calls, 4, "one failed write, then one retried write");
    let (creates, puts): (Vec<_>, Vec<_>) = accepted
        .iter()
        .partition(|(parts, _)| parts.uri.path() != "/data");
    assert_eq!(creates.len(), 2);
    assert_eq!(
        creates[0].0.uri, creates[1].0.uri,
        "the retry writes the same path"
    );
    assert_eq!(puts.len(), 1, "the payload is delivered once");
    assert_eq!(puts[0].1, Bytes::from("{\"message\":\"hello\"}\n"));
}

#[tokio::test]
async fn webhdfs_does_not_retry_permanent_write_error() {
    let (accepted, calls) = write_through_sink(StatusCode::FORBIDDEN).await;

    assert_eq!(calls, 2, "a permanent error must not be retried");
    assert!(
        accepted
            .iter()
            .all(|(parts, _)| parts.uri.path() != "/data")
    );
}
