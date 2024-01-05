use std::{
    collections::HashMap,
    fmt,
    task::{ready, Context, Poll},
};

use aws_sdk_cloudwatchlogs::{
    operation::{
        create_log_group::CreateLogGroupError, create_log_stream::CreateLogStreamError,
        describe_log_streams::DescribeLogStreamsError, put_log_events::PutLogEventsError,
        put_retention_policy::PutRetentionPolicyError,
    },
    types::InputLogEvent,
    Client as CloudwatchLogsClient,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use chrono::Duration;
use futures::{future::BoxFuture, FutureExt};
use futures_util::TryFutureExt;
use http::{
    header::{HeaderName, InvalidHeaderName, InvalidHeaderValue},
    HeaderValue,
};
use indexmap::IndexMap;
use snafu::{ResultExt, Snafu};
use tokio::sync::oneshot;
use tower::{
    buffer::Buffer,
    limit::{ConcurrencyLimit, RateLimit},
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};
use vector_lib::stream::DriverResponse;
use vector_lib::{
    finalization::EventStatus,
    request_metadata::{GroupedCountByteSize, MetaDescriptive},
};

use crate::sinks::{
    aws_cloudwatch_logs::{
        config::CloudwatchLogsSinkConfig, config::Retention, request, retry::CloudwatchRetryLogic,
        sink::BatchCloudwatchRequest, CloudwatchKey,
    },
    util::{retries::FibonacciRetryPolicy, EncodedLength, TowerRequestSettings},
};

type Svc = Buffer<
    ConcurrencyLimit<
        RateLimit<
            Retry<
                FibonacciRetryPolicy<CloudwatchRetryLogic<()>>,
                Buffer<Timeout<CloudwatchLogsSvc>, Vec<InputLogEvent>>,
            >,
        >,
    >,
    Vec<InputLogEvent>,
>;

#[derive(Debug)]
pub enum CloudwatchError {
    Put(SdkError<PutLogEventsError, HttpResponse>),
    DescribeLogStreams(SdkError<DescribeLogStreamsError, HttpResponse>),
    CreateStream(SdkError<CreateLogStreamError, HttpResponse>),
    CreateGroup(SdkError<CreateLogGroupError, HttpResponse>),
    PutRetentionPolicy(SdkError<PutRetentionPolicyError, HttpResponse>),
    NoStreamsFound,
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CloudwatchError::Put(error) => write!(f, "CloudwatchError::Put: {}", error),
            CloudwatchError::DescribeLogStreams(error) => {
                write!(f, "CloudwatchError::DescribeLogStreams: {}", error)
            }
            CloudwatchError::CreateStream(error) => {
                write!(f, "CloudwatchError::CreateStream: {}", error)
            }
            CloudwatchError::CreateGroup(error) => {
                write!(f, "CloudwatchError::CreateGroup: {}", error)
            }
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
            CloudwatchError::PutRetentionPolicy(error) => {
                write!(f, "CloudwatchError::PutRetentionPolicy: {}", error)
            }
        }
    }
}

impl std::error::Error for CloudwatchError {}

impl From<SdkError<PutLogEventsError, HttpResponse>> for CloudwatchError {
    fn from(error: SdkError<PutLogEventsError, HttpResponse>) -> Self {
        CloudwatchError::Put(error)
    }
}

impl From<SdkError<DescribeLogStreamsError, HttpResponse>> for CloudwatchError {
    fn from(error: SdkError<DescribeLogStreamsError, HttpResponse>) -> Self {
        CloudwatchError::DescribeLogStreams(error)
    }
}

#[derive(Debug)]
pub struct CloudwatchResponse {
    events_byte_size: GroupedCountByteSize,
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

    fn events_sent(&self) -> &GroupedCountByteSize {
        &self.events_byte_size
    }
}

#[derive(Snafu, Debug)]
enum HeaderError {
    #[snafu(display("invalid header name {source}"))]
    InvalidName { source: InvalidHeaderName },
    #[snafu(display("invalid header value {source}"))]
    InvalidValue { source: InvalidHeaderValue },
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(
        config: CloudwatchLogsSinkConfig,
        client: CloudwatchLogsClient,
    ) -> crate::Result<Self> {
        let request_settings = config.request.tower.into_settings();

        let headers = config
            .request
            .headers
            .iter()
            .map(|(name, value)| {
                Ok((
                    HeaderName::from_bytes(name.as_bytes()).context(InvalidNameSnafu {})?,
                    HeaderValue::from_str(value.as_str()).context(InvalidValueSnafu {})?,
                ))
            })
            .collect::<Result<IndexMap<_, _>, HeaderError>>()?;

        Ok(Self {
            config,
            clients: HashMap::new(),
            request_settings,
            client,
            headers,
        })
    }
}

impl Service<BatchCloudwatchRequest> for CloudwatchLogsPartitionSvc {
    type Response = CloudwatchResponse;
    type Error = crate::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut req: BatchCloudwatchRequest) -> Self::Future {
        let metadata = std::mem::take(req.metadata_mut());
        let events_byte_size = metadata.into_events_estimated_json_encoded_byte_size();

        let key = req.key;
        let events = req
            .events
            .into_iter()
            .map(|req| {
                InputLogEvent::builder()
                    .message(req.message)
                    .timestamp(req.timestamp)
                    .build()
                    .expect("all builder fields specified")
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
                    self.config.clone(),
                    &key,
                    self.client.clone(),
                    self.headers.clone(),
                ));

            self.clients.insert(key, svc.clone());
            svc
        };

        svc.oneshot(events)
            .map_ok(move |_x| CloudwatchResponse { events_byte_size })
            .map_err(Into::into)
            .boxed()
    }
}

impl CloudwatchLogsSvc {
    pub fn new(
        config: CloudwatchLogsSinkConfig,
        key: &CloudwatchKey,
        client: CloudwatchLogsClient,
        headers: IndexMap<HeaderName, HeaderValue>,
    ) -> Self {
        let group_name = key.group.clone();
        let stream_name = key.stream.clone();

        let create_missing_group = config.create_missing_group;
        let create_missing_stream = config.create_missing_stream;

        let retention = config.retention.clone();

        CloudwatchLogsSvc {
            headers,
            client,
            stream_name,
            group_name,
            create_missing_group,
            create_missing_stream,
            retention,
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
                self.headers.clone(),
                self.stream_name.clone(),
                self.group_name.clone(),
                self.create_missing_group,
                self.create_missing_stream,
                self.retention.clone(),
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
    client: CloudwatchLogsClient,
    headers: IndexMap<HeaderName, HeaderValue>,
    stream_name: String,
    group_name: String,
    create_missing_group: bool,
    create_missing_stream: bool,
    retention: Retention,
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
    headers: IndexMap<HeaderName, HeaderValue>,
    clients: HashMap<CloudwatchKey, Svc>,
    request_settings: TowerRequestSettings,
    client: CloudwatchLogsClient,
}
