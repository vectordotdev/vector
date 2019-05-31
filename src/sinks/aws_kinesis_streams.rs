use crate::{
    buffers::Acker,
    event::{self, Event},
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, SinkExt,
    },
};
use futures::{Future, Poll, Sink};
use rand::random;
use rusoto_core::RusotoFuture;
use rusoto_kinesis::{
    Kinesis, KinesisClient, ListStreamsInput, PutRecordsError, PutRecordsInput, PutRecordsOutput,
    PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, fmt, sync::Arc, time::Duration};
use tokio_trace::field;
use tokio_trace_futures::{Instrument, Instrumented};
use tower::{Service, ServiceBuilder};

#[derive(Clone)]
pub struct KinesisService {
    client: Arc<KinesisClient>,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub batch_size: Option<usize>,
    pub batch_timeout: Option<u64>,
    pub encoding: Option<Encoding>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[typetag::serde(name = "aws_kinesis_streams")]
impl crate::topology::config::SinkConfig for KinesisSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let config = self.clone();
        let sink = KinesisService::new(config, acker)?;
        let healthcheck = healthcheck(self.clone())?;
        Ok((Box::new(sink), healthcheck))
    }
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        acker: Acker,
    ) -> Result<impl Sink<SinkItem = Event, SinkError = ()>, String> {
        let client = Arc::new(KinesisClient::new(config.region.clone().try_into()?));

        let batch_size = config.batch_size.unwrap_or(bytesize::mib(5u64) as usize);
        let batch_timeout = config.batch_timeout.unwrap_or(1);

        let timeout = config.request_timeout_secs.unwrap_or(30);
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(1);
        let rate_limit_duration = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(10);
        let retry_attempts = config.request_retry_attempts.unwrap_or(5);
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);
        let encoding = config.encoding.clone();

        let policy = FixedRetryPolicy::new(
            retry_attempts,
            Duration::from_secs(retry_backoff_secs),
            KinesisRetryLogic,
        );

        let kinesis = KinesisService { client, config };

        let svc = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .rate_limit(rate_limit_num, Duration::from_secs(rate_limit_duration))
            .retry(policy)
            .timeout(Duration::from_secs(timeout))
            .service(kinesis);

        let sink = BatchServiceSink::new(svc, acker)
            .batched_with_min(Vec::new(), batch_size, Duration::from_secs(batch_timeout))
            .with(move |e| encode_event(e, &encoding));

        Ok(sink)
    }

    fn gen_partition_key(&mut self) -> String {
        random::<[char; 16]>()
            .into_iter()
            .fold(String::new(), |mut s, c| {
                s.push(*c);
                s
            })
    }
}

impl Service<Vec<Vec<u8>>> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = PutRecordsError;
    type Future = Instrumented<RusotoFuture<PutRecordsOutput, PutRecordsError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Vec<u8>>) -> Self::Future {
        debug!(
            message = "sending events.",
            events = &field::debug(items.len())
        );

        let records = items
            .into_iter()
            .map(|data| PutRecordsRequestEntry {
                data,
                partition_key: self.gen_partition_key(),
                ..Default::default()
            })
            .collect();

        let request = PutRecordsInput {
            records,
            stream_name: self.config.stream_name.clone(),
        };

        self.client
            .put_records(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct KinesisRetryLogic;

impl RetryLogic for KinesisRetryLogic {
    type Error = PutRecordsError;
    type Response = PutRecordsOutput;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            PutRecordsError::ProvisionedThroughputExceeded(_) => true,
            PutRecordsError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

fn healthcheck(config: KinesisSinkConfig) -> Result<super::Healthcheck, String> {
    let client = KinesisClient::new(config.region.try_into()?);
    let stream_name = config.stream_name;

    let fut = client
        .list_streams(ListStreamsInput {
            exclusive_start_stream_name: Some(stream_name.clone()),
            limit: Some(1),
        })
        .map_err(|e| format!("ListStreams failed: {}", e))
        .and_then(move |res| Ok(res.stream_names.into_iter().next()))
        .and_then(move |name| {
            if let Some(name) = name {
                if name == stream_name {
                    Ok(())
                } else {
                    Err(format!(
                        "Stream names do not match, got {}, expected {}",
                        name, stream_name
                    ))
                }
            } else {
                Err(format!(
                    "Stream returned does not contain any streams that match {}",
                    stream_name
                ))
            }
        });

    Ok(Box::new(fut))
}

fn encode_event(event: Event, encoding: &Option<Encoding>) -> Result<Vec<u8>, ()> {
    let log = event.into_log();

    match (encoding, log.is_structured()) {
        (&Some(Encoding::Json), _) | (_, true) => {
            serde_json::to_vec(&log.all_fields()).map_err(|e| panic!("Error encoding: {}", e))
        }
        (&Some(Encoding::Text), _) | (_, false) => {
            let bytes = log[&event::MESSAGE].as_bytes().to_vec();
            Ok(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{self, Event};
    use std::collections::HashMap;

    #[test]
    fn kinesis_encode_event_non_structured() {
        let message = "hello world".to_string();
        let bytes = encode_event(message.clone().into(), &None).unwrap();

        assert_eq!(&bytes[..], message.as_bytes())
    }

    #[test]
    fn kinesis_encode_event_structured() {
        let message = "hello world".to_string();
        let mut event = Event::from(message.clone());
        event
            .as_mut_log()
            .insert_explicit("key".into(), "value".into());
        let bytes = encode_event(event, &None).unwrap();

        let map: HashMap<String, String> = serde_json::from_slice(&bytes[..]).unwrap();

        assert_eq!(map[&event::MESSAGE.to_string()], message);
        assert_eq!(map["key"], "value".to_string())
    }
}

#[cfg(feature = "kinesis-integration-tests")]
#[cfg(test)]
mod integration_tests {

    use crate::buffers::Acker;
    use crate::region::RegionOrEndpoint;
    use crate::sinks::aws_kinesis_streams::{KinesisService, KinesisSinkConfig};
    use crate::test_util::random_lines_with_stream;
    use futures::{Future, Sink};
    use rusoto_core::Region;
    use rusoto_kinesis::{Kinesis, KinesisClient};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    const STREAM_NAME: &'static str = "RouterTest";

    #[test]
    fn kinesis_put_records() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4568".into(),
        };

        ensure_stream(region.clone());

        std::thread::sleep(std::time::Duration::from_secs(1));

        let config = KinesisSinkConfig {
            stream_name: STREAM_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:4568".into()),
            batch_size: Some(2),
            ..Default::default()
        };

        let mut rt = Runtime::new().unwrap();

        let sink = KinesisService::new(config, Acker::Null).unwrap();

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        rt.block_on(pump).unwrap();

        std::thread::sleep(std::time::Duration::from_secs(1));

        let timestamp = timestamp as f64 / 1000.0;
        let records = rt
            .block_on(fetch_records(STREAM_NAME.into(), timestamp, region))
            .unwrap();

        let output_lines = records
            .into_iter()
            .map(|e| String::from_utf8(e.data).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines)
    }

    fn fetch_records(
        stream_name: String,
        timestamp: f64,
        region: Region,
    ) -> impl Future<Item = Vec<rusoto_kinesis::Record>, Error = ()> {
        let client = Arc::new(KinesisClient::new(region));

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

    fn ensure_stream(region: Region) {
        let client = KinesisClient::new(region);

        let req = rusoto_kinesis::CreateStreamInput {
            stream_name: STREAM_NAME.into(),
            shard_count: 1,
        };

        match client.create_stream(req).sync() {
            Ok(_) => (),
            Err(e) => println!("Unable to check the stream {:?}", e),
        };
    }

}
