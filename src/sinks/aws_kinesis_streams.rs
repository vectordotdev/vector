use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::Event,
    internal_events::AwsKinesisStreamsEventSent,
    rusoto::{self, AWSAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::RetryLogic,
        sink::Response,
        BatchConfig, BatchSettings, Compression, EncodedLength, TowerRequestConfig, VecBuffer,
    },
};
use bytes::Bytes;
use futures::{future::BoxFuture, stream, FutureExt, Sink, SinkExt, StreamExt, TryFutureExt};
use lazy_static::lazy_static;
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

#[derive(Clone)]
pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub partition_key_field: Option<String>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AWSAuthentication,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        timeout_secs: Some(30),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<KinesisSinkConfig>("aws_kinesis_streams")
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
#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let client = self.create_client()?;
        let healthcheck = self.clone().healthcheck(client.clone()).boxed();
        let sink = KinesisService::new(self.clone(), client, cx)?;
        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_streams"
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

    fn create_client(&self) -> crate::Result<KinesisClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client()?;
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
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
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
                        emit!(AwsKinesisStreamsEventSent { byte_size });
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
) -> Option<PutRecordsRequestEntry> {
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

    encoding.apply_rules(&mut event);

    let log = event.into_log();
    let data = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    Some(PutRecordsRequestEntry {
        data: Bytes::from(data),
        partition_key,
        ..Default::default()
    })
}

fn gen_partition_key() -> String {
    random::<[char; 16]>()
        .iter()
        .fold(String::new(), |mut s, c| {
            s.push(*c);
            s
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{event::Event, test_util::random_string};
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<KinesisSinkConfig>();
    }

    #[test]
    fn kinesis_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &None, &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], message.as_bytes());
    }

    #[test]
    fn kinesis_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event.as_mut_log().insert("key", "value");
        let event = encode_event(event, &None, &Encoding::Json.into()).unwrap();

        let map: BTreeMap<String, String> = serde_json::from_slice(&event.data[..]).unwrap();

        assert_eq!(map[&log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "some_key");
        let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], b"hello world");
        assert_eq!(&event.partition_key, &"some_key".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key_limit() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", random_string(300));
        let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], b"hello world");
        assert_eq!(event.partition_key.len(), 256);
    }

    #[test]
    fn kinesis_encode_event_apply_rules() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "some_key");

        let mut encoding: EncodingConfig<_> = Encoding::Json.into();
        encoding.except_fields = Some(vec!["key".into()]);

        let event = encode_event(event, &Some("key".into()), &encoding).unwrap();
        let map: BTreeMap<String, String> = serde_json::from_slice(&event.data[..]).unwrap();

        assert_eq!(&event.partition_key, &"some_key".to_string());
        assert!(!map.contains_key("key"));
    }
}

#[cfg(feature = "aws-kinesis-streams-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::SinkContext,
        rusoto::RegionOrEndpoint,
        test_util::{random_lines_with_stream, random_string},
    };
    use rusoto_core::Region;
    use rusoto_kinesis::{Kinesis, KinesisClient};
    use std::sync::Arc;
    use tokio::time::{delay_for, Duration};

    #[tokio::test]
    async fn kinesis_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4566".into(),
        };

        ensure_stream(region.clone(), stream.clone()).await;

        let config = KinesisSinkConfig {
            stream_name: stream.clone(),
            partition_key_field: None,
            region: RegionOrEndpoint::with_endpoint("http://localhost:4566".into()),
            encoding: Encoding::Text.into(),
            compression: Compression::None,
            batch: BatchConfig {
                max_events: Some(2),
                ..Default::default()
            },
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let cx = SinkContext::new_test();

        let client = config.create_client().unwrap();
        let mut sink = KinesisService::new(config, client, cx).unwrap();

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (mut input_lines, events) = random_lines_with_stream(100, 11);
        let mut events = events.map(Ok);

        let _ = sink.send_all(&mut events).await.unwrap();

        delay_for(Duration::from_secs(1)).await;

        let timestamp = timestamp as f64 / 1000.0;
        let records = fetch_records(stream, timestamp, region).await.unwrap();

        let mut output_lines = records
            .into_iter()
            .map(|e| String::from_utf8(e.data.to_vec()).unwrap())
            .collect::<Vec<_>>();

        input_lines.sort();
        output_lines.sort();
        assert_eq!(output_lines, input_lines)
    }

    async fn fetch_records(
        stream_name: String,
        timestamp: f64,
        region: Region,
    ) -> crate::Result<Vec<rusoto_kinesis::Record>> {
        let client = Arc::new(KinesisClient::new(region));

        let req = rusoto_kinesis::DescribeStreamInput {
            stream_name: stream_name.clone(),
            ..Default::default()
        };
        let resp = client.describe_stream(req).await?;
        let shard = resp
            .stream_description
            .shards
            .into_iter()
            .next()
            .expect("No shards");

        let req = rusoto_kinesis::GetShardIteratorInput {
            stream_name,
            shard_id: shard.shard_id,
            shard_iterator_type: "AT_TIMESTAMP".into(),
            timestamp: Some(timestamp),
            ..Default::default()
        };
        let resp = client.get_shard_iterator(req).await?;
        let shard_iterator = resp.shard_iterator.expect("No iterator age produced");

        let req = rusoto_kinesis::GetRecordsInput {
            shard_iterator,
            // limit: Some(limit),
            limit: None,
        };
        let resp = client.get_records(req).await?;
        Ok(resp.records)
    }

    async fn ensure_stream(region: Region, stream_name: String) {
        let client = KinesisClient::new(region);

        let req = rusoto_kinesis::CreateStreamInput {
            stream_name,
            shard_count: 1,
        };

        match client.create_stream(req).await {
            Ok(_) => (),
            Err(error) => panic!("Unable to check the stream {:?}", error),
        };

        // Wait for localstack to persist stream, otherwise it returns ResourceNotFound errors
        // during PutRecords
        //
        // I initially tried using `wait_for` with `DescribeStream` but localstack would
        // successfully return the stream before it was able to accept PutRecords requests
        delay_for(Duration::from_secs(1)).await;
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
