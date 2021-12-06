use crate::sinks::aws_cloudwatch_logs::config::CloudwatchLogsSinkConfig;
use crate::sinks::aws_cloudwatch_logs::retry::CloudwatchRetryLogic;
use crate::sinks::aws_cloudwatch_logs::{request, CloudwatchKey};
use crate::sinks::util::retries::FixedRetryPolicy;
use crate::sinks::util::{
    EncodedLength, PartitionInnerBuffer, TowerRequestConfig, TowerRequestSettings,
};
use chrono::Duration;
use chrono::Utc;
use futures::future::BoxFuture;
use futures::{ready, FutureExt};
use futures_util::TryFutureExt;
use rusoto_core::RusotoError;
use rusoto_logs::{
    CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError, DescribeLogStreamsError,
    InputLogEvent, PutLogEventsError,
};
use std::collections::HashMap;
use std::fmt;
use std::task::{Context, Poll};
use tokio::sync::oneshot;
use tower::buffer::Buffer;
use tower::limit::{ConcurrencyLimit, RateLimit};
use tower::retry::Retry;
use tower::timeout::Timeout;
use tower::{Service, ServiceBuilder, ServiceExt};

type Svc = Buffer<
    ConcurrencyLimit<
        RateLimit<
            Retry<
                FixedRetryPolicy<CloudwatchRetryLogic>,
                Buffer<Timeout<CloudwatchLogsSvc>, Vec<InputLogEvent>>,
            >,
        >,
    >,
    Vec<InputLogEvent>,
>;

#[derive(Debug)]
pub enum CloudwatchError {
    Put(RusotoError<PutLogEventsError>),
    Describe(RusotoError<DescribeLogStreamsError>),
    CreateStream(RusotoError<CreateLogStreamError>),
    CreateGroup(RusotoError<CreateLogGroupError>),
    NoStreamsFound,
    ServiceDropped,
    MakeService,
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudwatchError::Put(error) => write!(f, "CloudwatchError::Put: {}", error),
            CloudwatchError::Describe(error) => write!(f, "CloudwatchError::Describe: {}", error),
            CloudwatchError::CreateStream(error) => {
                write!(f, "CloudwatchError::CreateStream: {}", error)
            }
            CloudwatchError::CreateGroup(error) => {
                write!(f, "CloudwatchError::CreateGroup: {}", error)
            }
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
            CloudwatchError::ServiceDropped => write!(
                f,
                "CloudwatchError: The service was dropped while there was a request in flight."
            ),
            CloudwatchError::MakeService => write!(
                f,
                "CloudwatchError: The inner service was unable to be created."
            ),
        }
    }
}

impl std::error::Error for CloudwatchError {}

impl From<RusotoError<PutLogEventsError>> for CloudwatchError {
    fn from(error: RusotoError<PutLogEventsError>) -> Self {
        CloudwatchError::Put(error)
    }
}

impl From<RusotoError<DescribeLogStreamsError>> for CloudwatchError {
    fn from(error: RusotoError<DescribeLogStreamsError>) -> Self {
        CloudwatchError::Describe(error)
    }
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(config: CloudwatchLogsSinkConfig, client: CloudWatchLogsClient) -> Self {
        let request_settings = config.request.unwrap_with(&TowerRequestConfig::default());

        Self {
            config,
            clients: HashMap::new(),
            request_settings,
            client,
        }
    }
}

impl Service<PartitionInnerBuffer<Vec<InputLogEvent>, CloudwatchKey>>
    for CloudwatchLogsPartitionSvc
{
    type Response = ();
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(
        &mut self,
        req: PartitionInnerBuffer<Vec<InputLogEvent>, CloudwatchKey>,
    ) -> Self::Future {
        let (events, key) = req.into_parts();

        let svc = if let Some(svc) = &mut self.clients.get_mut(&key) {
            svc.clone()
        } else {
            // Concurrency limit is 1 because we need token from previous request.
            let svc = ServiceBuilder::new()
                .buffer(1)
                .concurrency_limit(1)
                .rate_limit(
                    self.request_settings.rate_limit_num,
                    self.request_settings.rate_limit_duration,
                )
                .retry(self.request_settings.retry_policy(CloudwatchRetryLogic))
                .buffer(1)
                .timeout(self.request_settings.timeout)
                .service(CloudwatchLogsSvc::new(
                    &self.config,
                    &key,
                    self.client.clone(),
                ));

            self.clients.insert(key, svc.clone());
            svc
        };

        svc.oneshot(events).map_err(Into::into).boxed()
    }
}

impl CloudwatchLogsSvc {
    pub fn new(
        config: &CloudwatchLogsSinkConfig,
        key: &CloudwatchKey,
        client: CloudWatchLogsClient,
    ) -> Self {
        let group_name = key.group.clone();
        let stream_name = key.stream.clone();

        let create_missing_group = config.create_missing_group.unwrap_or(true);
        let create_missing_stream = config.create_missing_stream.unwrap_or(true);

        CloudwatchLogsSvc {
            client,
            stream_name,
            group_name,
            create_missing_group,
            create_missing_stream,
            token: None,
            token_rx: None,
        }
    }

    pub fn process_events(&self, events: Vec<InputLogEvent>) -> Vec<Vec<InputLogEvent>> {
        let now = Utc::now();
        // Acceptable range of Event timestamps.
        let age_range = (now - Duration::days(14)).timestamp_millis()
            ..(now + Duration::hours(2)).timestamp_millis();

        let mut events = events
            .into_iter()
            .filter(|e| age_range.contains(&e.timestamp))
            .collect::<Vec<_>>();

        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp);

        info!(message = "Sending events.", events = %events.len());

        let mut event_batches = Vec::new();
        if events.is_empty() {
            // This should happen rarely.
            event_batches.push(Vec::new());
        } else {
            // We will split events into 24h batches.
            // Relies on log_events being sorted by timestamp in ascending order.
            while let Some(oldest) = events.first() {
                let limit = oldest.timestamp + Duration::days(1).num_milliseconds();

                if events.last().expect("Events can't be empty").timestamp <= limit {
                    // Fast path.
                    // In most cases the difference between oldest and newest event
                    // is less than 24h.
                    event_batches.push(events);
                    break;
                }

                // At this point we know that an event older than the limit exists.
                //
                // We will find none or one of the events with timestamp==limit.
                // In the case of more events with limit, we can just split them
                // at found event, and send those before at with this batch, and
                // those after at with the next batch.
                let at = events
                    .binary_search_by_key(&limit, |e| e.timestamp)
                    .unwrap_or_else(|at| at);

                // Can't be empty
                let remainder = events.split_off(at);
                event_batches.push(events);
                events = remainder;
            }
        }

        event_batches
    }
}

impl Service<Vec<InputLogEvent>> for CloudwatchLogsSvc {
    type Response = ();
    type Error = CloudwatchError;
    type Future = request::CloudwatchFuture;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        if let Some(rx) = &mut self.token_rx {
            match ready!(rx.poll_unpin(cx)) {
                Ok(token) => {
                    self.token = token;
                    self.token_rx = None;
                }
                Err(_) => {
                    // This case only happens when the `tx` end gets dropped due to an error
                    // in this case we just reset the token and try again.
                    self.token = None;
                    self.token_rx = None;
                }
            }
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Vec<InputLogEvent>) -> Self::Future {
        if self.token_rx.is_none() {
            let event_batches = self.process_events(req);

            let (tx, rx) = oneshot::channel();
            self.token_rx = Some(rx);

            request::CloudwatchFuture::new(
                self.client.clone(),
                self.stream_name.clone(),
                self.group_name.clone(),
                self.create_missing_group,
                self.create_missing_stream,
                event_batches,
                self.token.take(),
                tx,
            )
        } else {
            panic!("poll_ready was not called; this is a bug!");
        }
    }
}

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    stream_name: String,
    group_name: String,
    create_missing_group: bool,
    create_missing_stream: bool,
    token: Option<String>,
    token_rx: Option<oneshot::Receiver<Option<String>>>,
}

impl EncodedLength for InputLogEvent {
    fn encoded_length(&self) -> usize {
        self.message.len() + 26
    }
}

#[derive(Clone)]
pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
    request_settings: TowerRequestSettings,
    client: CloudWatchLogsClient,
}
