use crate::{
    dns::Resolver,
    event::{self, Event},
    region::RegionOrEndpoint,
    sinks::util::{retries::RetryLogic, BatchConfig, SinkExt, TowerRequestConfig},
    topology::config::{DataType, SinkConfig, SinkContext, SinkDescription},
};
use bytes::Bytes;
use futures::{stream::iter_ok, Future, Poll, Sink};
use lazy_static::lazy_static;
use rusoto_core::{Region, RusotoError, RusotoFuture};
use rusoto_firehose::{
    KinesisFirehose, KinesisFirehoseClient, ListDeliveryStreamsInput, PutRecordBatchError,
    PutRecordBatchInput, PutRecordBatchOutput, Record,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{convert::TryInto, fmt, sync::Arc};
use tower::Service;
use tracing_futures::{Instrument, Instrumented};

#[derive(Clone)]
pub struct KinesisFirehoseService {
    client: Arc<KinesisFirehoseClient>,
    config: KinesisFirehoseSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct KinesisFirehoseSinkConfig {
    pub stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: Encoding,
    #[serde(default, flatten)]
    pub batch: BatchConfig,
    #[serde(flatten)]
    pub request: TowerRequestConfig,
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig = TowerRequestConfig {
        request_timeout_secs: Some(30),
        ..Default::default()
    };
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Derivative)]
#[serde(rename_all = "snake_case")]
#[derivative(Default)]
pub enum Encoding {
    #[derivative(Default)]
    Text,
    Json,
}

inventory::submit! {
    SinkDescription::new::<KinesisFirehoseSinkConfig>("aws_kinesis_firehose")
}

#[typetag::serde(name = "aws_kinesis_firehose")]
impl SinkConfig for KinesisFirehoseSinkConfig {
    fn build(&self, cx: SinkContext) -> crate::Result<(super::RouterSink, super::Healthcheck)> {
        let config = self.clone();
        let healthcheck = healthcheck(self.clone(), cx.resolver())?;
        let sink = KinesisFirehoseService::new(config, cx)?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_kinesis_firehose"
    }
}

impl KinesisFirehoseService {
    pub fn new(
        config: KinesisFirehoseSinkConfig,
        cx: SinkContext,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let client = Arc::new(create_client(
            config.region.clone().try_into()?,
            cx.resolver(),
        )?);

        let batch = config.batch.unwrap_or(bytesize::mib(1u64), 1);
        let request = config.request.unwrap_with(&REQUEST_DEFAULTS);
        let encoding = config.encoding.clone();

        let kinesis = KinesisFirehoseService { client, config };

        let sink = request
            .batch_sink(KinesisFirehoseRetryLogic, kinesis, cx.acker())
            .batched_with_min(Vec::new(), &batch)
            .with_flat_map(move |e| iter_ok(encode_event(e, &encoding)));

        Ok(sink)
    }
}

impl Service<Vec<Record>> for KinesisFirehoseService {
    type Response = PutRecordBatchOutput;
    type Error = RusotoError<PutRecordBatchError>;
    type Future = Instrumented<RusotoFuture<PutRecordBatchOutput, PutRecordBatchError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, records: Vec<Record>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %records.len(),
        );

        let request = PutRecordBatchInput {
            records,
            delivery_stream_name: self.config.stream_name.clone(),
        };

        self.client
            .put_record_batch(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for KinesisFirehoseService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisFirehoseService")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct KinesisFirehoseRetryLogic;

impl RetryLogic for KinesisFirehoseRetryLogic {
    type Error = RusotoError<PutRecordBatchError>;
    type Response = PutRecordBatchOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            RusotoError::HttpDispatch(_) => true,
            RusotoError::Service(PutRecordBatchError::ServiceUnavailable(_)) => true,
            RusotoError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("ListDeliveryStreams failed: {}", source))]
    ListDeliveryStreamsFailed {
        source: RusotoError<rusoto_firehose::ListDeliveryStreamsError>,
    },
    #[snafu(display("Stream names do not match, got {}, expected {}", name, stream_name))]
    StreamNamesMismatch { name: String, stream_name: String },
    #[snafu(display(
        "Stream returned does not contain any streams that match {}",
        stream_name
    ))]
    NoMatchingStreamName { stream_name: String },
}

fn healthcheck(
    config: KinesisFirehoseSinkConfig,
    resolver: Resolver,
) -> crate::Result<super::Healthcheck> {
    let client = create_client(config.region.try_into()?, resolver)?;
    let stream_name = config.stream_name;

    let fut = client
        .list_delivery_streams(ListDeliveryStreamsInput {
            exclusive_start_delivery_stream_name: Some(stream_name.clone()),
            limit: Some(1),
            delivery_stream_type: None,
        })
        .map_err(|source| HealthcheckError::ListDeliveryStreamsFailed { source }.into())
        .and_then(move |res| Ok(res.delivery_stream_names.into_iter().next()))
        .and_then(move |name| {
            if let Some(name) = name {
                if name == stream_name {
                    Ok(())
                } else {
                    Err(HealthcheckError::StreamNamesMismatch { name, stream_name }.into())
                }
            } else {
                Err(HealthcheckError::NoMatchingStreamName { stream_name }.into())
            }
        });

    Ok(Box::new(fut))
}

fn create_client(region: Region, resolver: Resolver) -> crate::Result<KinesisFirehoseClient> {
    use rusoto_credential::DefaultCredentialsProvider;

    let p = DefaultCredentialsProvider::new()?;
    let d = crate::sinks::util::rusoto::client(resolver)?;

    Ok(KinesisFirehoseClient::new_with(d, p, region))
}

fn encode_event(event: Event, encoding: &Encoding) -> Option<Record> {
    let log = event.into_log();
    let data = match encoding {
        Encoding::Json => {
            serde_json::to_vec(&log.unflatten()).expect("Error encoding event as json.")
        }

        Encoding::Text => log
            .get(&event::MESSAGE)
            .map(|v| v.as_bytes().to_vec())
            .unwrap_or_default(),
    };

    let data = Bytes::from(data);

    Some(Record { data })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{self, Event},
        test_util::random_string,
    };
    use std::collections::HashMap;

    #[test]
    fn kinesis_encode_event_text() {
        let message = "hello world".to_string();
        let event = encode_event(message.clone().into(), &None, &Encoding::Text).unwrap();

        assert_eq!(&event.data[..], message.as_bytes());
    }

    #[test]
    fn kinesis_encode_event_json() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());
        let event = encode_event(event, &None, &Encoding::Json).unwrap();

        let map: HashMap<String, String> = serde_json::from_slice(&event.data[..]).unwrap();

        assert_eq!(map[&event::MESSAGE.to_string()], message);
        assert_eq!(map["key"], "value".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("key".into(), "some_key".into());
        let event = encode_event(event, &Some("key".into()), &Encoding::Text).unwrap();

        assert_eq!(&event.data[..], "hello world".as_bytes());
        assert_eq!(&event.partition_key, &"some_key".to_string());
    }

    #[test]
    fn kinesis_encode_event_custom_partition_key_limit() {
        let mut event = Event::from("hello world");
        event
            .as_mut_log()
            .insert_implicit("key".into(), random_string(300).into());
        let event = encode_event(event, &Some("key".into()), &Encoding::Text).unwrap();

        assert_eq!(&event.data[..], "hello world".as_bytes());
        assert_eq!(event.partition_key.len(), 256);
    }
}

#[cfg(feature = "kinesis-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        region::RegionOrEndpoint,
        runtime,
        test_util::{random_lines_with_stream, random_string},
        topology::config::SinkContext,
    };
    use futures::{Future, Sink};
    use rusoto_core::Region;
    use rusoto_kinesis::{KinesisFirehose, KinesisFirehoseClient};
    use std::sync::Arc;

    #[test]
    fn kinesis_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4568".into(),
        };

        ensure_stream(region.clone(), stream.clone());

        let config = KinesisFirehoseSinkConfig {
            stream_name: stream.clone(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4568".into()),
            batch: BatchConfig {
                batch_size: Some(2),
                batch_timeout: None,
            },
            ..Default::default()
        };

        let mut rt = runtime::Runtime::new().unwrap();
        let cx = SinkContext::new_test(rt.executor());

        let sink = KinesisFirehoseService::new(config, cx).unwrap();

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (mut input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let _ = rt.block_on(pump).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let timestamp = timestamp as f64 / 1000.0;
        let records = rt
            .block_on(fetch_records(stream.clone(), timestamp, region))
            .unwrap();

        let mut output_lines = records
            .into_iter()
            .map(|e| String::from_utf8(e.data.to_vec()).unwrap())
            .collect::<Vec<_>>();

        input_lines.sort();
        output_lines.sort();
        assert_eq!(output_lines, input_lines)
    }

    fn fetch_records(
        stream_name: String,
        timestamp: f64,
        region: Region,
    ) -> impl Future<Item = Vec<rusoto_kinesis::Record>, Error = ()> {
        let client = Arc::new(KinesisFirehoseClient::new(region));

        let stream_name1 = stream_name.clone();
        let describe = rusoto_kinesis::DescribeStreamInput {
            stream_name,
            ..Default::default()
        };

        let client1 = client.clone();
        let client2 = client.clone();

        client
            .describe_stream(describe)
            .map_err(|e| panic!("{:?}", e))
            .map(|res| {
                res.stream_description
                    .shards
                    .into_iter()
                    .next()
                    .expect("No shards")
            })
            .map(|shard| shard.shard_id)
            .and_then(move |shard_id| {
                let req = rusoto_kinesis::GetShardIteratorInput {
                    stream_name: stream_name1,
                    shard_id,
                    shard_iterator_type: "AT_TIMESTAMP".into(),
                    timestamp: Some(timestamp),
                    ..Default::default()
                };

                client1
                    .get_shard_iterator(req)
                    .map_err(|e| panic!("{:?}", e))
            })
            .map(|iter| iter.shard_iterator.expect("No iterator age produced"))
            .and_then(move |shard_iterator| {
                let req = rusoto_kinesis::GetRecordsInput {
                    shard_iterator,
                    // limit: Some(limit),
                    limit: None,
                };

                client2.get_records(req).map_err(|e| panic!("{:?}", e))
            })
            .map(|records| records.records)
    }

    fn ensure_stream(region: Region, stream_name: String) {
        let client = KinesisFirehoseClient::new(region);

        let req = rusoto_kinesis::CreateStreamInput {
            stream_name,
            shard_count: 1,
        };

        match client.create_stream(req).sync() {
            Ok(_) => (),
            Err(e) => println!("Unable to check the stream {:?}", e),
        };
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
