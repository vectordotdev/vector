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
    pub region: String,
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
        let region = config.region.clone().parse::<Region>().unwrap();
        let client = Arc::new(KinesisClient::new(region));

        let batch_size = config.batch_size;
        let service = KinesisService { client, config };

        let policy = RetryPolicy::new(5, Duration::from_secs(1));

        let service = Timeout::new(service, Duration::from_secs(5));
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
