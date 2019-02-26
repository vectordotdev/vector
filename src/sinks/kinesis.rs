use super::Record;
use crate::sinks::util::batch::{BatchSink, VecBatcher};
use futures::{future, Poll, Sink};
use rand::random;
use rusoto_core::{Region, RusotoFuture};
use rusoto_kinesis::{
    Kinesis, KinesisClient, PutRecordsError, PutRecordsInput, PutRecordsOutput,
    PutRecordsRequestEntry,
};
use serde::{Deserialize, Serialize};
use std::error::Error as StdError;
use std::fmt;
use std::time::Duration;
use tower_buffer::Buffer;
use tower_retry::{Policy, Retry};
use tower_service::Service;
use tower_timeout::Timeout;

type Request = Vec<Vec<u8>>;
type Error = tower_buffer::error::Error<tower_timeout::Error<PutRecordsError>>;

pub struct KinesisService {
    client: KinesisClient,
    config: KinesisSinkConfig,
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct KinesisSinkConfig {
    pub stream_name: String,
    pub region: String,
    pub batch_size: usize,
}

#[derive(Debug, Clone)]
struct RetryPolicy {
    attempts: usize,
}

impl KinesisService {
    pub fn new(config: KinesisSinkConfig) -> impl Sink<SinkItem = Record, SinkError = ()> {
        let region = config.region.clone().parse::<Region>().unwrap();
        let client = KinesisClient::new(region);

        let batcher = VecBatcher::new(config.batch_size);
        let service = KinesisService { client, config };

        let policy = RetryPolicy { attempts: 5 };

        let service = Timeout::new(service, Duration::from_secs(5));
        let service = Buffer::new(service, 1).unwrap();
        let service = Retry::new(policy, service);

        BatchSink::new(batcher, service)
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

impl Policy<Request, PutRecordsOutput, Error> for RetryPolicy {
    type Future = future::FutureResult<Self, ()>;

    fn retry(
        &self,
        _: &Request,
        response: Result<&PutRecordsOutput, &Error>,
    ) -> Option<Self::Future> {
        match response {
            Ok(_) => None,
            // TODO: clean this up, they are all options so they should just return none
            Err(error) => match error
                .source()
                .unwrap()
                .downcast_ref::<PutRecordsError>()
                .unwrap()
            {
                PutRecordsError::ProvisionedThroughputExceeded(_) => {
                    let policy = RetryPolicy {
                        attempts: self.attempts - 1,
                    };
                    Some(future::ok(policy))
                }
                _ => None,
            },
        }
    }

    fn clone_request(&self, request: &Request) -> Option<Request> {
        Some(request.clone())
    }
}
