use crate::{
    dns::Resolver,
    event::{self, Event},
    internal_events::AwsKinesisStreamsEventSent,
    region::RegionOrEndpoint,
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries2::RetryLogic,
        rusoto,
        service2::TowerRequestConfig,
        sink::Response,
        BatchEventsConfig,
    },
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes05::Bytes;
use futures::{future::BoxFuture, FutureExt, TryFutureExt};
use futures01::{stream::iter_ok, Sink};
use lazy_static::lazy_static;
use rand::random;
use rusoto_core::{Region, RusotoError};
use rusoto_kinesis::{
    DescribeStreamInput, Kinesis, KinesisClient, PutRecordsError, PutRecordsInput,
    PutRecordsOutput, PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    convert::TryInto,
    fmt,
    sync::Arc,
    task::{Context, Poll},
};
use string_cache::DefaultAtom as Atom;
use tower03::Service;
use tracing_futures::Instrument;

#[derive(Clone)]
pub struct KinesisService {
    client: Arc<KinesisClient>,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub partition_key_field: Option<Atom>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    #[serde(default)]
    pub batch: BatchEventsConfig,
    #[serde(default)]
    pub request: TowerRequestConfig,
    pub assume_role: Option<String>,
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
    SinkDescription::new_without_default::<KinesisSinkConfig>("aws_kinesis_streams")
}

#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let healthcheck = healthcheck(self.clone(), cx.resolver()).boxed().compat();
        let sink = KinesisService::new(self.clone(), cx)?;
        Ok((Box::new(sink), Box::new(healthcheck)))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_streams"
    }
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let client = Arc::new(create_client(
            (&config.region).try_into()?,
            config.assume_role.clone(),
            cx.resolver(),
        )?);

        let batch = config.batch.unwrap_or(500, 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();
        let partition_key_field = config.partition_key_field.clone();

        let kinesis = KinesisService { client, config };

        let sink = request
            .batch_sink(KinesisRetryLogic, kinesis, Vec::new(), batch, cx.acker())
            .sink_map_err(|e| error!("Fatal kinesis streams sink error: {}", e))
            .with_flat_map(move |e| iter_ok(encode_event(e, &partition_key_field, &encoding)));

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
            message = "sending records.",
            events = %records.len(),
        );

        let client = self.client.clone();
        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        Box::pin(async move { client.put_records(request).await }.instrument(info_span!("request")))
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
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
            RusotoError::HttpDispatch(_) => true,
            RusotoError::Service(PutRecordsError::ProvisionedThroughputExceeded(_)) => true,
            RusotoError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
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

async fn healthcheck(config: KinesisSinkConfig, resolver: Resolver) -> crate::Result<()> {
    let client = create_client(
        config.region.try_into()?,
        config.assume_role.clone(),
        resolver,
    )?;
    let stream_name = config.stream_name;

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

fn create_client(
    region: Region,
    assume_role: Option<String>,
    resolver: Resolver,
) -> crate::Result<KinesisClient> {
    let client = rusoto::client(resolver)?;
    let creds = rusoto::AwsCredentialsProvider::new(&region, assume_role)?;

    Ok(KinesisClient::new_with(client, creds, region))
}

fn encode_event(
    mut event: Event,
    partition_key_field: &Option<Atom>,
    encoding: &EncodingConfig<Encoding>,
) -> Option<PutRecordsRequestEntry> {
    encoding.apply_rules(&mut event);
    let partition_key = if let Some(partition_key_field) = partition_key_field {
        if let Some(v) = event.as_log().get(&partition_key_field) {
            v.to_string_lossy()
        } else {
            warn!(
                message = "Partition key does not exist; Dropping event.",
                %partition_key_field,
                rate_limit_secs = 30,
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

    let log = event.into_log();
    let data = match encoding.codec() {
        Encoding::Json => serde_json::to_vec(&log).expect("Error encoding event as json."),
        Encoding::Text => log
            .get(&event::log_schema().message_key())
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    let data = Bytes::from(data);

    emit!(AwsKinesisStreamsEventSent {
        byte_size: data.len()
    });
    Some(PutRecordsRequestEntry {
        data,
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
    use crate::{
        event::{self, Event},
        test_util::random_string,
    };
    use std::collections::BTreeMap;

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

        assert_eq!(map[&event::log_schema().message_key().to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", "some_key");
        let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], "hello world".as_bytes());
        assert_eq!(&event.partition_key, &"some_key".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key_limit() {
        let mut event = Event::from("hello world");
        event.as_mut_log().insert("key", random_string(300));
        let event = encode_event(event, &Some("key".into()), &Encoding::Text.into()).unwrap();

        assert_eq!(&event.data[..], "hello world".as_bytes());
        assert_eq!(event.partition_key.len(), 256);
    }
}

#[cfg(feature = "aws-kinesis-streams-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        region::RegionOrEndpoint,
        test_util::{random_lines_with_stream, random_string, runtime},
        topology::config::SinkContext,
    };
    use futures01::Sink;
    use rusoto_core::Region;
    use rusoto_kinesis::{Kinesis, KinesisClient};
    use std::sync::Arc;

    #[test]
    fn kinesis_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4568".into(),
        };

        let mut rt = runtime();
        rt.block_on_std(ensure_stream(region.clone(), stream.clone()));

        let config = KinesisSinkConfig {
            stream_name: stream.clone(),
            partition_key_field: None,
            region: RegionOrEndpoint::with_endpoint("http://localhost:4568".into()),
            encoding: Encoding::Text.into(),
            batch: BatchEventsConfig {
                max_events: Some(2),
                timeout_secs: None,
            },
            request: Default::default(),
            assume_role: None,
        };

        let cx = SinkContext::new_test(rt.executor());

        let sink = KinesisService::new(config, cx).unwrap();

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (mut input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let timestamp = timestamp as f64 / 1000.0;
        let records = rt
            .block_on_std(fetch_records(stream, timestamp, region))
            .unwrap();

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
            Err(e) => panic!("Unable to check the stream {:?}", e),
        };
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
