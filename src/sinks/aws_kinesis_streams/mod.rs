mod integration_tests;
mod tests;
mod config;

use crate::{
    config::{
        log_schema, DataType, GenerateConfig, ProxyConfig, SinkConfig, SinkContext, SinkDescription,
    },
    event::Event,
    internal_events::AwsKinesisStreamsEventSent,
    rusoto::{self, AwsAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::{self, Response},
        BatchConfig, BatchSettings, Compression, EncodedEvent, EncodedLength, TowerRequestConfig,
        VecBuffer,
    },
};
use bytes::Bytes;
use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use rand::random;
use rusoto_core::RusotoError;
use rusoto_kinesis::{
    DescribeStreamInput, Kinesis, KinesisClient, PutRecordsError, PutRecordsInput,
    PutRecordsOutput, PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryInto,
    fmt,
    task::{Context, Poll},
};
use tower::Service;
use tracing_futures::Instrument;
use vector_core::ByteSizeOf;



#[derive(Clone)]
pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}





inventory::submit! {
    SinkDescription::new::<KinesisSinkConfig>("sinks.aws_kinesis_streams")
}

impl GenerateConfig for KinesisSinkConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(
            r#"region = "us-east-1"
            stream_name = "my-stream"
            encoding.codec = "json""#,
        )
            .unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "sinks.aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client(&cx.proxy)?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = KinesisService::new(self.clone(), client, cx)?;
        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "sinks.aws_kinesis_streams"
    }
}

impl KinesisSinkConfig {
    async fn healthcheck(self, client: KinesisClient) -> crate::Result<()> {
        let stream_name = self.stream_name;

        let req = client.describe_stream(DescribeStreamInput {
            stream_name: stream_name.clone(),
            exclusive_start_shard_id: None,
            limit: Some(1),
        });

        match req.await {
            Ok(resp) => {
                let name = resp.stream_description.stream_name;
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            }
            Err(source) => Err(HealthcheckError::DescribeStreamFailed { source }.into()),
        }
    }

    fn create_client(&self, proxy: &ProxyConfig) -> crate::Result<KinesisClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client(proxy)?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(KinesisClient::new_with_client(client, region))
    }
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        client: KinesisClient,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<Event, Error = ()>> {
        let batch = BatchSettings::default()
            .bytes(5_000_000)
            .events(500)
            .timeout(1)
            .parse_config(config.batch)?;
        let request = config.request.unwrap_with(&TowerRequestConfig::default());
        let encoding = config.encoding.clone();
        let partition_key_field = config.partition_key_field.clone();

        let kinesis = KinesisService { client, config };

        let sink = request
            .batch_sink(
                KinesisRetryLogic,
                kinesis,
                VecBuffer::new(batch.size),
                batch.timeout,
                cx.acker(),
                sink::StdServiceLogic::default(),
            )
            .sink_map_err(|error| error!(message = "Fatal kinesis streams sink error.", %error))
            .with_flat_map(move |e| {
                stream::iter(encode_event(e, &partition_key_field, &encoding)).map(Ok)
            });

        Ok(sink)
    }
}

impl Service<Vec<PutRecordsRequestEntry>> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = RusotoError<PutRecordsError>;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, records: Vec<PutRecordsRequestEntry>) -> Self::Future {
        debug!(
            message = "Sending records.",
            events = %records.len(),
        );

        let sizes: Vec<usize> = records.iter().map(|record| record.data.len()).collect();

        let client = self.client.clone();
        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        Box::pin(async move {
            client
                .put_records(request)
                .inspect_ok(|_| {
                    for byte_size in sizes {
                        emit!(&AwsKinesisStreamsEventSent { byte_size });
                    }
                })
                .instrument(info_span!("request"))
                .await
        })
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}

impl EncodedLength for PutRecordsRequestEntry {
    fn encoded_length(&self) -> usize {
        // data is base64 encoded
        (self.data.len() + 2) / 3 * 4
            + self
            .explicit_hash_key
            .as_ref()
            .map(|s| s.len())
            .unwrap_or_default()
            + self.partition_key.len()
            + 10
    }
}

impl Response for PutRecordsOutput {}

#[derive(Debug, Clone)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = RusotoError<PutRecordsError>;
    type Response = PutRecordsOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::Service(PutRecordsError::ProvisionedThroughputExceeded(_)) => true,
            error => rusoto::is_retriable_error(error),
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeStream failed: {}", source))]
    DescribeStreamFailed {
        source: RusotoError<rusoto_kinesis::DescribeStreamError>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
    #[snafu(display(
    "Stream returned does not contain any streams that match {}",
    stream_name
    ))]
    NoMatchingStreamName { stream_name: String },
}

fn encode_event(
    mut event: Event,
    partition_key_field: &Option<String>,
    encoding: &EncodingConfig<Encoding>,
) -> Option<EncodedEvent<PutRecordsRequestEntry>> {
    let partition_key = if let Some(partition_key_field) = partition_key_field {
        if let Some(v) = event.as_log().get(&partition_key_field) {
            v.to_string_lossy()
        } else {
            warn!(
                message = "Partition key does not exist; dropping event.",
                %partition_key_field,
                internal_log_rate_secs = 30,
            );
            return None;
        }
    } else {
        gen_partition_key()
    };

    let partition_key = if partition_key.len() >= 256 {
        partition_key[..256].to_string()
    } else {
        partition_key
    };

    let byte_size = event.size_of();
    encoding.apply_rules(&mut event);

    let log = event.into_log();
    let data = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    Some(EncodedEvent::new(
        PutRecordsRequestEntry {
            data: Bytes::from(data),
            partition_key,
            ..Default::default()
        },
        byte_size,
    ))
}

fn gen_partition_key() -> String {
    random::<[char; 16]>()
        .iter()
        .fold(String::new(), |mut s, c| {
            s.push(*c);
            s
        })
}



