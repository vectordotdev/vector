use std::{
    collections::HashMap,
    fmt,
    task::{Context, Poll},
};

use chrono::Duration;
use futures::{future::BoxFuture, ready, FutureExt};
use futures_util::TryFutureExt;
use rusoto_core::RusotoError;
use rusoto_logs::{
    CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError, DescribeLogStreamsError,
    InputLogEvent, PutLogEventsError,
};
use tokio::sync::oneshot;
use tower::{
    buffer::Buffer,
    limit::{ConcurrencyLimit, RateLimit},
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};
use vector_core::{internal_event::EventsSent, stream::DriverResponse};
use vrl::prelude::fmt::Debug;

use crate::{
    event::EventStatus,
    sinks::{
        aws_cloudwatch_logs::{
            config::CloudwatchLogsSinkConfig, request, retry::CloudwatchRetryLogic,
            sink::BatchCloudwatchRequest, CloudwatchKey,
        },
        util::{
            retries::FixedRetryPolicy, EncodedLength, TowerRequestConfig, TowerRequestSettings,
        },
    },
};

type Svc = Buffer<
    ConcurrencyLimit<
        RateLimit<
            Retry<
                FixedRetryPolicy<CloudwatchRetryLogic<()>>,
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

#[derive(Debug)]
pub struct CloudwatchResponse {
    events_count: usize,
    events_byte_size: usize,
}

impl crate::sinks::util::sink::Response for CloudwatchResponse {
    fn is_successful(&self) -> bool {
        true
    }

    fn is_transient(&self) -> bool {
        false
    }
}

impl DriverResponse for CloudwatchResponse {
    fn event_status(&self) -> EventStatus {
        EventStatus::Delivered
    }

    fn events_sent(&self) -> EventsSent {
        EventsSent {
            count: self.events_count,
            byte_size: self.events_byte_size,
            output: None,
        }
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

impl Service<BatchCloudwatchRequest> for CloudwatchLogsPartitionSvc {
    type Response = CloudwatchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: BatchCloudwatchRequest) -> Self::Future {
        let events_count = req.events.len();
        let events_byte_size = req.events.iter().map(|x| x.event_byte_size).sum();
        let key = req.key;
        let events = req
            .events
            .into_iter()
            .map(|req| InputLogEvent {
                message: req.message,
                timestamp: req.timestamp,
            })
            .collect();
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
                .retry(
                    self.request_settings
                        .retry_policy(CloudwatchRetryLogic::new()),
                )
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

        svc.oneshot(events)
            .map_ok(move |_x| CloudwatchResponse {
                events_count,
                events_byte_size,
            })
            .map_err(Into::into)
            .boxed()
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

    pub fn process_events(&self, mut events: Vec<InputLogEvent>) -> Vec<Vec<InputLogEvent>> {
        // Sort by timestamp
        events.sort_by_key(|e| e.timestamp);

        info!(message = "Sending events.", events = %events.len());

        let mut event_batches = Vec::new();

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

        event_batches
    }
}

impl Service<Vec<InputLogEvent>> for CloudwatchLogsSvc {
    type Response = ();
    type Error = CloudwatchError;
    type Future = request::CloudwatchFuture;

    fn poll_ready(&mut self, cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        if let Some(rx) = &mut self.token_rx {
            self.token = ready!(rx.poll_unpin(cx)).ok().flatten();
            self.token_rx = None;
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
