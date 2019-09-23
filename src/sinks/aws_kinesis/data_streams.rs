use crate::{
    buffers::Acker,
    event::Event,
    region::RegionOrEndpoint,
    sinks::{
        util::retries::{FixedRetryPolicy, RetryLogic},
        Healthcheck, RouterSink,
    },
    topology::config::{DataType, SinkConfig},
};
use futures::{Future, Poll, Sink};
use rand::random;
use rusoto_core::RusotoFuture;
use rusoto_kinesis::{
    DescribeStreamError::{self, ResourceNotFound},
    DescribeStreamInput, Kinesis, KinesisClient, PutRecordsError, PutRecordsInput,
    PutRecordsOutput, PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use std::{convert::TryInto, fmt, sync::Arc, time::Duration};
use string_cache::DefaultAtom as Atom;
use tower::Service;
use tracing_futures::{Instrument, Instrumented};

use super::{CoreSinkConfig, Encoding, TowerRequestConfig};

#[derive(Clone)]
pub struct KinesisService {
    stream_name: String,
    client: Arc<KinesisClient>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct KinesisConfig {
    pub stream_name: String,
    pub partition_key_field: Option<Atom>,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct KinesisSinkConfig {
    #[serde(default, flatten)]
    pub core_config: CoreSinkConfig,
    #[serde(flatten)]
    pub kinesis_config: KinesisConfig,
    #[serde(default, rename = "request")]
    pub request_config: TowerRequestConfig,
}

#[typetag::serde(name = "aws_kinesis_streams")]
impl SinkConfig for KinesisSinkConfig {
    fn build(&self, acker: Acker) -> crate::Result<(RouterSink, Healthcheck)> {
        let config = self.clone();
        let sink = KinesisService::new(config, acker)?;
        let healthcheck = healthcheck(self.kinesis_config.clone())?;
        Ok((Box::new(sink), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

impl KinesisService {
    pub fn new(
        config: KinesisSinkConfig,
        acker: Acker,
    ) -> crate::Result<impl Sink<SinkItem = Event, SinkError = ()>> {
        let kinesis_config = config.kinesis_config;
        let request_config = config.request_config;
        let core_config = config.core_config;

        let client = Arc::new(KinesisClient::new(
            kinesis_config.region.clone().try_into()?,
        ));

        let policy = FixedRetryPolicy::new(
            request_config.retry_attempts,
            Duration::from_secs(request_config.retry_backoff_secs),
            KinesisRetryLogic,
        );

        let kinesis = KinesisService {
            stream_name: kinesis_config.stream_name,
            client,
        };

        let partition_key_field = kinesis_config.partition_key_field;

        super::construct(
            core_config,
            request_config,
            kinesis,
            acker,
            policy,
            move |e, enc| encode_event(e, &partition_key_field, enc),
        )
    }
}

fn encode_event(
    event: Event,
    partition_key_field: &Option<Atom>,
    encoding: &Encoding,
) -> Option<PutRecordsRequestEntry> {
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

    let data = super::encode_event(event, encoding);

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

impl Service<Vec<PutRecordsRequestEntry>> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = PutRecordsError;
    type Future = Instrumented<RusotoFuture<PutRecordsOutput, PutRecordsError>>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, records: Vec<PutRecordsRequestEntry>) -> Self::Future {
        debug!(
            message = "sending records.",
            events = %records.len(),
        );

        let request = PutRecordsInput {
            records,
            stream_name: self.stream_name.clone(),
        };

        self.client
            .put_records(request)
            .instrument(info_span!("request"))
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("stream_name", &self.stream_name)
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
            PutRecordsError::HttpDispatch(_) => true,
            PutRecordsError::ProvisionedThroughputExceeded(_) => true,
            PutRecordsError::Unknown(res) if res.status.is_server_error() => true,
            _ => false,
        }
    }
}

type HealthcheckError = super::HealthcheckError<DescribeStreamError>;

fn healthcheck(config: KinesisConfig) -> crate::Result<crate::sinks::Healthcheck> {
    let client = KinesisClient::new(config.region.try_into()?);
    let stream_name = config.stream_name;

    let fut = client
        .describe_stream(DescribeStreamInput {
            exclusive_start_shard_id: None,
            limit: Some(1),
            stream_name,
        })
        .map_err(|source| match source {
            ResourceNotFound(resource) => HealthcheckError::NoMatchingStreamName {
                stream_name: resource,
            }
            .into(),
            other => HealthcheckError::StreamRetrievalFailed { source: other }.into(),
        })
        .and_then(move |res| {
            let description = res.stream_description;
            let status = &description.stream_status[..];

            match status {
                "CREATING" | "DELETING" => Err(HealthcheckError::StreamIsNotReady {
                    stream_name: description.stream_name,
                }
                .into()),
                _ => Ok(()),
            }
        });

    Ok(Box::new(fut))
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
        buffers::Acker,
        region::RegionOrEndpoint,
        test_util::{random_lines_with_stream, random_string},
    };
    use futures::{Future, Sink};
    use rusoto_core::Region;
    use rusoto_kinesis::{Kinesis, KinesisClient};
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[test]
    fn kinesis_put_records() {
        let stream = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:4568".into(),
        };

        ensure_stream(region.clone(), stream.clone());

        let config = KinesisSinkConfig {
            core_config: CoreSinkConfig {
                batch_size: 2,
                ..Default::default()
            },
            kinesis_config: KinesisConfig {
                stream_name: stream.clone(),
                region: RegionOrEndpoint::with_endpoint("http://localhost:4568".into()),
                ..Default::default()
            },
            request_config: Default::default(),
        };

        let mut rt = Runtime::new().unwrap();

        let sink = KinesisService::new(config, Acker::Null).unwrap();

        let timestamp = chrono::Utc::now().timestamp_millis();

        let (mut input_lines, events) = random_lines_with_stream(100, 11);

        std::thread::sleep(std::time::Duration::from_secs(1));

        let pump = sink.send_all(events);
        rt.block_on(pump).unwrap();

        let timestamp = timestamp as f64 / 1000.0;
        let records = rt
            .block_on(fetch_records(stream.clone(), timestamp, region))
            .unwrap();

        let mut output_lines = records
            .into_iter()
            .map(|e| String::from_utf8(e.data).unwrap())
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

    fn ensure_stream(region: Region, stream_name: String) {
        let client = KinesisClient::new(region);

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
