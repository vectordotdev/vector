use std::{ffi::OsStr, path::Path};

use bytes::Bytes;
use vector_lib::{
    codecs::{JsonSerializerConfig, NewlineDelimitedEncoderConfig, encoding::Framer},
    partition::Partitioner,
    request_metadata::GroupedCountByteSize,
};

use super::config::WebHdfsConfig;
use crate::{
    codecs::{Encoder, EncodingConfigWithFraming, SinkType},
    event::LogEvent,
    sinks::{
        opendal_common::{OpenDalRequest, OpenDalRequestBuilder},
        util::{
            Compression, HttpEndpoint, TowerRequestConfig,
            request_builder::{EncodeResult, RequestBuilder},
        },
    },
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

mod retry_tests {
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::Duration,
    };

    use tokio::time::timeout;
    use vector_lib::{
        codecs::{NewlineDelimitedEncoderConfig, TextSerializerConfig},
        event::{BatchNotifier, BatchStatus, Event, LogEvent},
    };
    use wiremock::{
        Mock, MockServer, Request, ResponseTemplate,
        matchers::{method, path_regex, query_param},
    };

    use super::default_config;
    use crate::{
        config::ValidatedSink,
        sinks::util::{Compression, HttpEndpoint},
    };

    async fn assert_webhdfs_delivery(
        failure_status: u16,
        failures_before_success: usize,
        retry_attempts: usize,
        expected_status: BatchStatus,
        expected_attempts: usize,
    ) {
        crate::test_util::trace_init();
        let server = MockServer::start().await;

        // Simulate the NameNode: tell the client where to upload the file.
        let server_uri = server.uri();

        Mock::given(method("PUT"))
            .and(query_param("op", "CREATE"))
            .respond_with(move |request: &Request| {
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "Location": format!(
                        "{}/upload{}",
                        server_uri,
                        request.url.path()
                    )
                }))
            })
            .mount(&server)
            .await;

        // Simulate the DataNode: fail the configured number of upload attempts.
        let attempts = AtomicUsize::new(0);

        Mock::given(method("PUT"))
            .and(path_regex(r"^/upload/"))
            .respond_with(move |_request: &Request| {
                let attempt = attempts.fetch_add(1, Ordering::Relaxed);

                if attempt < failures_before_success {
                    ResponseTemplate::new(failure_status).set_body_string("injected upload failure")
                } else {
                    ResponseTemplate::new(201)
                }
            })
            .mount(&server)
            .await;

        // Build the real sink using its production configuration path
        let mut config = default_config(
            (
                Some(NewlineDelimitedEncoderConfig::new()),
                TextSerializerConfig::default(),
            )
                .into(),
        );

        config.endpoint = HttpEndpoint::parse(&server.uri()).unwrap();
        config.root = "/".to_string();
        config.prefix = "retry_tests/".to_string();
        config.compression = Compression::None;
        config.batch.max_events = Some(3);
        config.batch.timeout_secs = Some(1.0);
        config.request.retry_attempts = retry_attempts;
        config.request.retry_jitter_mode = crate::sinks::util::retries::JitterMode::None;

        let validated = config.validate().unwrap();
        let operator = config.build_operator().unwrap();
        let sink = config.build_processor(operator, &validated).unwrap();

        // Three identifiable events should become one upload
        let messages = ["event-001", "event-002", "event-003"];

        let mut events: Vec<Event> = messages
            .iter()
            .map(|message| LogEvent::from(*message).into())
            .collect();

        // Observe the actual final delivery status of these events.
        let receiver = BatchNotifier::apply_to(&mut events);

        timeout(Duration::from_secs(30), sink.run_events(events))
            .await
            .expect("sink did not finish within 30 seconds")
            .expect("sink task failed");

        let status = timeout(Duration::from_secs(1), receiver)
            .await
            .expect("batch delivery status was never finalized");

        // Count data uploads, excluding the NameNode requests.
        let requests = server.received_requests().await.unwrap();

        let uploads: Vec<_> = requests
            .iter()
            .filter(|request| request.url.path().starts_with("/upload/"))
            .collect();

        assert_eq!(
            status,
            expected_status,
            "unexpected batch status; observed {} data upload(s)",
            uploads.len()
        );

        assert_eq!(
            uploads.len(),
            expected_attempts,
            "unexpected number of data uploads"
        );

        let expected_body: String = format!("{}\n", messages.join("\n"));

        for upload in &uploads {
            assert_eq!(
                upload.body.as_slice(),
                expected_body.as_bytes(),
                "upload contained missing, changed, or duplicate events"
            );

            assert_eq!(
                upload.url.path(),
                uploads[0].url.path(),
                "retry changed the destination filename"
            );
        }
    }

    #[tokio::test]
    async fn webhdfs_delivers_without_retry() {
        assert_webhdfs_delivery(503, 0, 2, BatchStatus::Delivered, 1).await;
    }

    #[tokio::test]
    async fn webhdfs_retries_transient_write_failure() {
        assert_webhdfs_delivery(503, 1, 2, BatchStatus::Delivered, 2).await;
    }

    #[tokio::test]
    async fn webhdfs_does_not_retry_permanent_write_failure() {
        assert_webhdfs_delivery(403, usize::MAX, 2, BatchStatus::Rejected, 1).await;
    }

    #[tokio::test]
    async fn webhdfs_stops_after_retry_budget_is_exhausted() {
        assert_webhdfs_delivery(503, usize::MAX, 2, BatchStatus::Rejected, 3).await;
    }

    #[tokio::test]
    async fn webhdfs_respects_zero_retries() {
        assert_webhdfs_delivery(503, 1, 0, BatchStatus::Rejected, 1).await;
    }
}
