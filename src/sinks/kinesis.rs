use super::Record;
use crate::sinks::util::batch::BatchSink;
use futures::{try_ready, Async, Future, Poll, Sink};
use log::{error, warn};
use rand::random;
use rusoto_core::{Region, RusotoFuture};
use rusoto_kinesis::{
    Kinesis, KinesisClient, PutRecordsError, PutRecordsInput, PutRecordsOutput,
    PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::timer::Delay;
use tower_retry::{Policy, Retry};
use tower_service::Service;
use tower_timeout::Timeout;

type Request = Vec<Vec<u8>>;
type Error = tower_timeout::Error<PutRecordsError>;

#[derive(Clone)]
pub struct KinesisService {
    client: Arc<KinesisClient>,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub region: Region,
    pub batch_size: usize,
}

#[derive(Debug, Clone)]
struct RetryPolicy {
    attempts: usize,
    backoff: Duration,
}

struct RetryPolicyFuture {
    delay: Delay,
    policy: RetryPolicy,
}

impl KinesisService {
    pub fn new(config: KinesisSinkConfig) -> impl Sink<SinkItem = Record, SinkError = ()> {
        // let region = config.region.clone().parse::<Region>().unwrap();
        let client = Arc::new(KinesisClient::new(config.region.clone()));

        let batch_size = config.batch_size;
        let service = KinesisService { client, config };

        let policy = RetryPolicy::new(5, Duration::from_secs(1));

        let service = Timeout::new(service, Duration::from_secs(10));
        let service = Retry::new(policy, service);

        BatchSink::new(service, batch_size)
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

impl Service<Request> for KinesisService {
    type Response = PutRecordsOutput;
    type Error = PutRecordsError;
    type Future = RusotoFuture<PutRecordsOutput, PutRecordsError>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, items: Vec<Vec<u8>>) -> Self::Future {
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

        self.client.put_records(request)
    }
}

impl fmt::Debug for KinesisService {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("KinesisService")
            .field("config", &self.config)
            .finish()
    }
}

impl RetryPolicy {
    pub fn new(attempts: usize, backoff: Duration) -> Self {
        RetryPolicy { attempts, backoff }
    }

    fn should_retry(&self, error: &PutRecordsError) -> bool {
        match error {
            PutRecordsError::ProvisionedThroughputExceeded(reason) => {
                warn!("Kinesis ProvisionedThroghPutExceeded: {}", reason);
                true
            }
            PutRecordsError::Unknown(ref res) => {
                if res.status.is_server_error() {
                    if let Ok(reason) = String::from_utf8(res.body.clone()) {
                        error!("Kinesis UnkownError Occured: {}", reason);
                    } else {
                        error!(
                            "Kinesis UnkownError Occured with status: {}",
                            res.status.as_u16()
                        );
                    }

                    true
                } else {
                    false
                }
            }
            _ => false,
        }
    }
}

impl Policy<Request, PutRecordsOutput, Error> for RetryPolicy {
    type Future = RetryPolicyFuture;

    fn retry(
        &self,
        _: &Request,
        response: Result<&PutRecordsOutput, &Error>,
    ) -> Option<Self::Future> {
        if self.attempts > 0 {
            if let Err(Some(ref err)) = response.map_err(|e| find::<PutRecordsError>(e)) {
                if self.should_retry(err) {
                    let policy = RetryPolicy::new(self.attempts - 1, self.backoff.clone());
                    let amt = Instant::now() + self.backoff;
                    let delay = Delay::new(amt);

                    return Some(RetryPolicyFuture { delay, policy });
                }
            }
        }

        None
    }

    fn clone_request(&self, request: &Request) -> Option<Request> {
        Some(request.clone())
    }
}

impl Future for RetryPolicyFuture {
    type Item = RetryPolicy;
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        try_ready!(self.delay.poll().map_err(|_| ()));
        Ok(Async::Ready(self.policy.clone()))
    }
}

// Need this for the moment to iterator `Box<dyn Error>`
fn find<'a, T: StdError + 'static>(mut e: &'a (dyn StdError + 'static)) -> Option<&'a T> {
    loop {
        if let Some(err) = e.downcast_ref::<T>() {
            return Some(err);
        }

        e = match e.source() {
            Some(e) => e,
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "kinesis-integration-tests")]

    use crate::sinks::kinesis::{KinesisService, KinesisSinkConfig};
    use crate::test_util::random_lines;
    use crate::Record;
    use futures::{future::poll_fn, stream, Future, Sink};
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
            region: region.clone(),
            batch_size: 2,
        };

        let mut rt = Runtime::new().unwrap();

        let sink = KinesisService::new(config);

        let timestamp = chrono::Utc::now().timestamp_millis();

        let input_lines = random_lines(100).take(11).collect::<Vec<_>>();

        let records = input_lines
            .iter()
            .map(|line| Record::from(line.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(records.into_iter()));

        let (mut sink, _) = rt.block_on(pump).unwrap();

        rt.block_on(poll_fn(move || sink.close())).unwrap();

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
