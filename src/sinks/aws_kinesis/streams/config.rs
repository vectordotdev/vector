use aws_sdk_kinesis::operation::{
    describe_stream::DescribeStreamError, put_records::PutRecordsError,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::FutureExt;
use snafu::Snafu;
use vector_lib::configurable::{component::GenerateConfig, configurable_component};

use super::{
    KinesisClient, KinesisError, KinesisRecord, KinesisResponse, KinesisSinkBaseConfig, build_sink,
    record::{KinesisStreamClient, KinesisStreamRecord},
    sink::BatchKinesisRequest,
};
use crate::{
    aws::{ClientBuilder, create_client, is_retriable_error},
    config::{AcknowledgementsConfig, Input, ProxyConfig, SinkConfig, SinkContext, ValidatedSink},
    sinks::{
        Healthcheck, VectorSink,
        prelude::*,
        util::{
            BatchConfig, SinkBatchSettings,
            retries::{RetryAction, RetryLogic},
        },
    },
};

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: SdkError<DescribeStreamError, HttpResponse>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
}

pub(super) struct KinesisClientBuilder;

impl ClientBuilder for KinesisClientBuilder {
    type Client = KinesisClient;

    fn build(&self, config: &aws_types::SdkConfig) -> Self::Client {
        KinesisClient::new(config)
    }
}

pub(super) const MAX_PAYLOAD_SIZE: usize = 5_000_000;
pub(super) const MAX_PAYLOAD_EVENTS: usize = 500;

#[derive(Clone, Copy, Debug, Default)]
pub struct KinesisDefaultBatchSettings;

impl SinkBatchSettings for KinesisDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(MAX_PAYLOAD_EVENTS);
    const MAX_BYTES: Option<usize> = Some(MAX_PAYLOAD_SIZE);
    const TIMEOUT_SECS: f64 = 1.0;
}

/// Configuration for the `aws_kinesis_streams` sink.
#[configurable_component(sink(
    "aws_kinesis_streams",
    "Publish logs to AWS Kinesis Streams topics."
))]
#[derive(Clone, Debug)]
pub struct KinesisStreamsSinkConfig {
    #[serde(flatten)]
    pub base: KinesisSinkBaseConfig,

    #[serde(default)]
    pub batch: BatchConfig<KinesisDefaultBatchSettings>,

    /// Combine multiple logs into each Kinesis record using newline or KPL framing.
    /// Separate from `batch`, which groups Kinesis records into API requests.
    #[serde(default)]
    pub aggregation: Option<super::super::newline::AggregationConfig>,
}

impl KinesisStreamsSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.base.stream_name;

        let describe_result = client
            .describe_stream()
            .stream_name(stream_name.clone())
            .set_exclusive_start_shard_id(None)
            .limit(1)
            .send()
            .await;

        match describe_result {
            Ok(resp) => {
                let name = resp
                    .stream_description
                    .map(|x| x.stream_name)
                    .unwrap_or_default();
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeStreamFailed { source }.into()),
        }
    }

    /// Creates the AWS client.
    ///
    /// # Errors
    /// Returns an error if region resolution, authentication, or HTTP client configuration fails.
    pub async fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        create_client::<KinesisClientBuilder>(
            &KinesisClientBuilder {},
            &self.base.auth,
            self.base.region.region(),
            self.base.region.endpoint(),
            proxy,
            self.base.tls.as_ref(),
            None,
        )
        .await
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisStreamsSinkConfig {
    fn input(&self) -> Input {
        self.base.input()
    }

    fn acknowledgements(&self) -> &AcknowledgementsConfig {
        self.base.acknowledgements()
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedKinesisStreams {
    batch_settings: BatcherSettings,
    aggregation: Option<super::super::aggregation::Settings>,
}

#[async_trait::async_trait]
impl ValidatedSink for KinesisStreamsSinkConfig {
    type Validated = ValidatedKinesisStreams;

    fn validate(&self) -> crate::Result<ValidatedKinesisStreams> {
        let aggregation = self
            .aggregation
            .as_ref()
            .map(|config| config.settings(self.base.encoding.config()))
            .transpose()?
            .flatten();
        let batch_settings = self
            .batch
            .validate()?
            .limit_max_bytes(MAX_PAYLOAD_SIZE)?
            .limit_max_events(MAX_PAYLOAD_EVENTS)?
            .into_batcher_settings()?;
        self.base.encoding.validate()?;

        Ok(ValidatedKinesisStreams {
            batch_settings,
            aggregation,
        })
    }

    async fn build(
        &self,
        validated: &ValidatedKinesisStreams,
        cx: SinkContext,
    ) -> crate::Result<(VectorSink, Healthcheck)> {
        let client = self.create_client(&cx.proxy).await?;
        if validated
            .aggregation
            .is_some_and(|settings| settings.format == super::super::aggregation::Format::Kpl)
        {
            // Mandatory even when optional healthchecks are disabled. The newer
            // distribution field is only returned by DescribeStreamSummary.
            client
                .describe_stream_summary()
                .stream_name(&self.base.stream_name)
                .customize()
                .interceptor(super::kpl_compatibility::KplCompatibility)
                .send()
                .await
                .map_err(|error| format!("KPL stream compatibility check failed: {error:?}"))?;
        }
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();

        let sink = build_sink::<
            KinesisStreamClient,
            KinesisRecord,
            KinesisStreamRecord,
            KinesisError,
            KinesisRetryLogic,
        >(
            &self.base,
            self.base.partition_key_field.clone(),
            validated.batch_settings,
            validated.aggregation,
            KinesisStreamClient { client },
            KinesisRetryLogic {
                retry_partial: self.base.request_retry_partial,
            },
        )?;

        Ok((sink, healthcheck))
    }
}

impl GenerateConfig for KinesisStreamsSinkConfig {
    fn generate_config() -> serde_json::Value {
        serde_yaml::from_str(indoc::indoc! {
            r"partition_key_field: foo
            stream_name: my-stream
            encoding:
              codec: json",
        })
        .unwrap()
    }
}
#[derive(Default, Clone)]
struct KinesisRetryLogic {
    retry_partial: bool,
}

impl RetryLogic for KinesisRetryLogic {
    type Error = SdkError<KinesisError, HttpResponse>;
    type Request = BatchKinesisRequest<KinesisStreamRecord>;
    type Response = KinesisResponse;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        if let SdkError::ServiceError(inner) = error {
            // Note that if the request partially fails (records sent to one
            // partition fail but the others do not, for example), Vector
            // does not retry. This line only covers a failure for the entire
            // request.
            //
            // https://github.com/vectordotdev/vector/issues/359
            if matches!(
                inner.err(),
                PutRecordsError::ProvisionedThroughputExceededException(_)
            ) {
                return true;
            }
        }
        is_retriable_error(error)
    }

    fn should_retry_response(&self, response: &Self::Response) -> RetryAction<Self::Request> {
        if response.failure_count > 0 && self.retry_partial && !response.failed_records.is_empty() {
            let failed_records = response.failed_records.clone();
            RetryAction::RetryPartial(Box::new(move |original_request| {
                let failed_events: Vec<_> = failed_records
                    .iter()
                    .filter_map(|r| original_request.events.get(r.index).cloned())
                    .collect();

                let metadata = RequestMetadata::from_batch(
                    failed_events.iter().map(|req| req.get_metadata().clone()),
                );

                BatchKinesisRequest {
                    events: failed_events,
                    metadata,
                }
            }))
        } else {
            RetryAction::Successful
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{aws::AwsAuthentication, sinks::util::TowerRequestConfig};
    use crate::{
        codecs::{Encoder, Transformer},
        event::{BatchNotifier, BatchStatus, Event, EventStatus, Finalizable, LogEvent},
        sinks::{
            aws_kinesis::{
                aggregation::Settings, newline::requests, request_builder::KinesisRequestBuilder,
                service::RecordResult, streams::KinesisStreamRecord,
            },
            util::{
                Compression,
                retries::{RetryAction, RetryLogic},
            },
        },
    };
    use futures::StreamExt;
    use std::{marker::PhantomData, time::Duration};
    use vector_lib::{
        codecs::TextSerializerConfig,
        request_metadata::{MetaDescriptive, RequestMetadata},
    };

    #[tokio::test]
    async fn partial_retry_retains_whole_aggregate_metadata_and_finalizers() {
        for format in [
            super::super::super::aggregation::Format::Newline,
            super::super::super::aggregation::Format::Kpl,
        ] {
            let mut statuses = Vec::new();
            let events = (0..6)
                .map(|i| {
                    let (batch, status) = BatchNotifier::new_with_receiver();
                    statuses.push(status);
                    Event::Log(LogEvent::from(i.to_string()).with_batch_notifier(&batch))
                })
                .collect::<Vec<_>>();
            let builder = KinesisRequestBuilder::<KinesisStreamRecord> {
                compression: Compression::None,
                encoder: (
                    Transformer::default(),
                    Encoder::<()>::new(TextSerializerConfig::default().build().into()),
                ),
                _phantom: PhantomData,
            };
            let records = requests(
                futures::stream::iter(events).boxed(),
                builder,
                None,
                Settings {
                    format,
                    max_events: 2,
                    max_bytes: 1000,
                    timeout: Duration::from_secs(1),
                },
            )
            .collect::<Vec<_>>()
            .await;
            let metadata = RequestMetadata::from_batch(
                records.iter().map(|record| record.get_metadata().clone()),
            );
            let mut original = super::BatchKinesisRequest {
                events: records,
                metadata,
            };
            let response = super::KinesisResponse {
                failure_count: 1,
                events_byte_size: GroupedCountByteSize::default(),
                failed_records: vec![RecordResult {
                    index: 1,
                    success: false,
                    error_code: None,
                    error_message: None,
                }],
            };
            let RetryAction::RetryPartial(select) = (super::KinesisRetryLogic {
                retry_partial: true,
            })
            .should_retry_response(&response) else {
                panic!("expected partial retry");
            };
            let retry = select(original.clone());
            assert_eq!(original.get_metadata().event_count(), 6);
            assert_eq!(retry.events.len(), 1);
            assert_eq!(retry.get_metadata().event_count(), 2);
            let record = &retry.events[0].record.record;
            match format {
                super::super::super::aggregation::Format::Newline => {
                    assert_eq!(record.data().as_ref(), b"2\n3\n");
                }
                super::super::super::aggregation::Format::Kpl => assert_eq!(
                    super::super::super::kpl::test_support::decode(
                        record.data().as_ref(),
                        record.partition_key().expect("aggregate has a key")
                    ),
                    [b"2".to_vec(), b"3".to_vec()]
                ),
            }
            assert_eq!(retry.events[0].finalizers.len(), 2);
            retry.events[0]
                .finalizers
                .update_status(EventStatus::Rejected);
            original
                .take_finalizers()
                .update_status(EventStatus::Delivered);
            drop((retry, original));
            for (i, status) in statuses.into_iter().enumerate() {
                assert_eq!(
                    status.await,
                    if i == 2 || i == 3 {
                        BatchStatus::Rejected
                    } else {
                        BatchStatus::Delivered
                    }
                );
            }
        }
    }

    #[tokio::test]
    async fn default_aggregation_rejects_auto_distribution_even_without_healthcheck_execution() {
        let uri = crate::test_util::http::spawn_blackhole_http_server(|_| async {
            Ok(http::Response::new(http_body_util::Full::new(bytes::Bytes::from_static(br#"{"StreamDescriptionSummary":{"StreamName":"logs","StreamARN":"arn:aws:kinesis:us-east-1:123456789012:stream/logs","StreamStatus":"ACTIVE","RetentionPeriodHours":24,"StreamCreationTimestamp":0,"EnhancedMonitoring":[],"OpenShardCount":1,"RecordDistributionStrategy":"AUTO"}}"#))))
        }).await;
        let config: KinesisStreamsSinkConfig = toml::from_str(&format!(
            "stream_name = 'logs'\nregion = 'us-east-1'\nendpoint = '{uri}'\nencoding.codec = 'json'\nauth.access_key_id = 'test'\nauth.secret_access_key = 'test'\n[aggregation]\nenabled = true\n"
        )).unwrap();
        let result =
            crate::config::SinkConfig::build(&config, crate::config::SinkContext::default()).await;
        assert!(
            result.is_err(),
            "KPL on AUTO must fail before starting the sink"
        );
    }

    #[tokio::test]
    async fn kpl_build_checks_chunked_summary_and_leaves_other_modes_offline() {
        for (strategy, allowed) in [
            (None, true),
            (Some(serde_json::json!("USER_PARTITION_KEY")), true),
            (Some(serde_json::json!("AUTO")), false),
            (Some(serde_json::json!("FUTURE")), false),
            (Some(serde_json::Value::Null), false),
            (Some(serde_json::json!(1)), false),
        ] {
            let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let mut summary = serde_json::json!({"StreamDescriptionSummary": {"StreamName": "logs", "StreamARN": "arn:aws:kinesis:us-east-1:123456789012:stream/logs", "StreamStatus": "ACTIVE", "RetentionPeriodHours": 24, "StreamCreationTimestamp": 0, "EnhancedMonitoring": [], "OpenShardCount": 1}});
            if let Some(strategy) = strategy {
                summary["StreamDescriptionSummary"]["RecordDistributionStrategy"] = strategy;
            }
            let body = serde_json::to_vec(&summary).unwrap();
            let uri = chunked_summary_endpoint(body, std::sync::Arc::clone(&calls)).await;
            for (enabled, format) in [(false, "kpl"), (true, "newline"), (true, "kpl")] {
                let config: KinesisStreamsSinkConfig = toml::from_str(&format!(
                    "stream_name = 'logs'\nregion = 'us-east-1'\nendpoint = '{uri}'\nencoding.codec = 'json'\nauth.access_key_id = 'test'\nauth.secret_access_key = 'test'\n[aggregation]\nenabled = {enabled}\nformat = '{format}'\n"
                )).unwrap();
                let is_kpl = enabled && format == "kpl";
                let result = crate::config::SinkConfig::build(
                    &config,
                    crate::config::SinkContext::default(),
                )
                .await;
                assert_eq!(
                    result.is_ok(),
                    !is_kpl || allowed,
                    "summary: {summary}, format: {format}, enabled: {enabled}"
                );
                assert_eq!(
                    calls.load(std::sync::atomic::Ordering::SeqCst),
                    usize::from(is_kpl)
                );
            }
        }
    }

    async fn chunked_summary_endpoint(
        body: Vec<u8>,
        calls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ) -> http::Uri {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let body = body.clone();
                let calls = std::sync::Arc::clone(&calls);
                tokio::spawn(async move {
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    let header_end = loop {
                        let size = socket.read(&mut buffer).await.unwrap();
                        assert_ne!(size, 0);
                        request.extend_from_slice(&buffer[..size]);
                        if let Some(end) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                            break end + 4;
                        }
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    assert!(headers.contains("DescribeStreamSummary"));
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap();
                    while request.len() < header_end + length {
                        let size = socket.read(&mut buffer).await.unwrap();
                        assert_ne!(size, 0);
                        request.extend_from_slice(&buffer[..size]);
                    }
                    calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/x-amz-json-1.1\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").await.unwrap();
                    for chunk in [&body[..body.len() / 2], &body[body.len() / 2..]] {
                        socket
                            .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                            .await
                            .unwrap();
                        socket.write_all(chunk).await.unwrap();
                        socket.write_all(b"\r\n").await.unwrap();
                        tokio::task::yield_now().await;
                    }
                    socket.write_all(b"0\r\n\r\n").await.unwrap();
                });
            }
        });
        format!("http://{address}/").parse().unwrap()
    }

    #[test]
    fn kpl_accepts_other_codecs_and_validates_event_limits() {
        for (enabled, max_events, valid) in [
            (true, 1000, true),
            (true, 1001, false),
            (true, 0, false),
            (false, 1001, true),
        ] {
            let config: KinesisStreamsSinkConfig = toml::from_str(&format!(
                "stream_name = 'logs'\nencoding.codec = 'json'\nencoding.json.pretty = true\n[aggregation]\nenabled = {enabled}\nformat = 'kpl'\nmax_events = {max_events}\n"
            )).unwrap();
            assert_eq!(
                config
                    .aggregation
                    .as_ref()
                    .unwrap()
                    .settings(config.base.encoding.config())
                    .is_ok(),
                valid
            );
        }
    }

    #[test]
    fn newline_aggregation_is_optional_and_validated() {
        let base = "stream_name = 'logs'\nencoding.codec = 'text'\n";
        let config: KinesisStreamsSinkConfig = toml::from_str(base).unwrap();
        assert!(config.aggregation.is_none());
        let config: KinesisStreamsSinkConfig = toml::from_str(&format!(
            "{base}\n[aggregation]\nenabled = true\nformat = 'newline'\nmax_events = 2\n"
        ))
        .unwrap();
        assert_eq!(
            config
                .aggregation
                .unwrap()
                .settings(config.base.encoding.config())
                .unwrap()
                .unwrap()
                .max_events,
            2
        );
        let config: KinesisStreamsSinkConfig = toml::from_str(&format!(
            "{base}\n[aggregation]\nenabled = true\nformat = 'newline'\nmax_events = 0\n"
        ))
        .unwrap();
        assert!(
            config
                .aggregation
                .unwrap()
                .settings(config.base.encoding.config())
                .is_err()
        );
    }

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KinesisStreamsSinkConfig>();
    }

    #[test]
    fn validate_produces_batch_settings() {
        let config = KinesisStreamsSinkConfig {
            aggregation: None,
            batch: BatchConfig::<KinesisDefaultBatchSettings>::default(),
            base: KinesisSinkBaseConfig {
                stream_name: String::from("test"),
                region: crate::aws::RegionOrEndpoint::with_both(
                    "us-east-1",
                    "http://localhost:4566",
                ),
                encoding: vector_lib::codecs::JsonSerializerConfig::default().into(),
                compression: Compression::None,
                request: TowerRequestConfig::default(),
                tls: None,
                auth: AwsAuthentication::default(),
                request_retry_partial: false,
                acknowledgements: AcknowledgementsConfig::default(),
                partition_key_field: None,
            },
        };

        let validated = config.validate().expect("validation should succeed");
        assert_eq!(validated.batch_settings.item_limit, MAX_PAYLOAD_EVENTS);
        assert_eq!(validated.batch_settings.size_limit, MAX_PAYLOAD_SIZE);
    }
}
