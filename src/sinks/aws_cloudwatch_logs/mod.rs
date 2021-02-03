mod request;

use crate::{
    config::{log_schema, DataType, GenerateConfig, SinkConfig, SinkContext, SinkDescription},
    event::{Event, LogEvent, Value},
    rusoto::{self, AWSAuthentication, RegionOrEndpoint},
    sinks::util::{
        encoding::{EncodingConfig, EncodingConfiguration},
        retries::{FixedRetryPolicy, RetryLogic},
        BatchConfig, BatchSettings, Compression, EncodedLength, PartitionBatchSink,
        PartitionBuffer, PartitionInnerBuffer, TowerRequestConfig, TowerRequestSettings, VecBuffer,
    },
    template::Template,
};
use chrono::{Duration, Utc};
use futures::{future::BoxFuture, ready, stream, FutureExt, SinkExt, StreamExt, TryFutureExt};
use lazy_static::lazy_static;
use rusoto_core::{request::BufferedHttpResponse, RusotoError};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogStreamError,
    DescribeLogGroupsRequest, DescribeLogStreamsError, InputLogEvent, PutLogEventsError,
};
use serde::{Deserialize, Serialize};
use snafu::Snafu;
use std::{
    collections::HashMap,
    convert::TryInto,
    fmt,
    task::{Context, Poll},
};
use tokio::sync::oneshot;
use tower::{
    buffer::Buffer,
    limit::{concurrency::ConcurrencyLimit, rate::RateLimit},
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};

// Estimated maximum size of InputLogEvent with an empty message
const EVENT_SIZE_OVERHEAD: usize = 50;
const MAX_EVENT_SIZE: usize = 256 * 1024;
const MAX_MESSAGE_SIZE: usize = MAX_EVENT_SIZE - EVENT_SIZE_OVERHEAD;

#[derive(Debug, Snafu)]
pub(self) enum CloudwatchLogsError {
    #[snafu(display("{}", source))]
    HttpClientError {
        source: rusoto_core::request::TlsError,
    },
    #[snafu(display("{}", source))]
    InvalidCloudwatchCredentials {
        source: rusoto_credential::CredentialsError,
    },
    #[snafu(display("Encoded event is too long, length={}", length))]
    EventTooLong { length: usize },
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CloudwatchLogsSinkConfig {
    pub group_name: Template,
    pub stream_name: Template,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub encoding: EncodingConfig<Encoding>,
    pub create_missing_group: Option<bool>,
    pub create_missing_stream: Option<bool>,
    #[serde(default)]
    pub compression: Compression,
    #[serde(default)]
    pub batch: BatchConfig,
    #[serde(default)]
    pub request: TowerRequestConfig<Option<usize>>,
    // Deprecated name. Moved to auth.
    assume_role: Option<String>,
    #[serde(default)]
    pub auth: AWSAuthentication,
}

inventory::submit! {
    SinkDescription::new::<CloudwatchLogsSinkConfig>("aws_cloudwatch_logs")
}

impl GenerateConfig for CloudwatchLogsSinkConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(default_config(Encoding::Json)).unwrap()
    }
}

fn default_config(e: Encoding) -> CloudwatchLogsSinkConfig {
    CloudwatchLogsSinkConfig {
        group_name: Default::default(),
        stream_name: Default::default(),
        region: Default::default(),
        encoding: e.into(),
        create_missing_group: Default::default(),
        create_missing_stream: Default::default(),
        compression: Default::default(),
        batch: Default::default(),
        request: Default::default(),
        assume_role: Default::default(),
        auth: Default::default(),
    }
}

lazy_static! {
    static ref REQUEST_DEFAULTS: TowerRequestConfig<Option<usize>> = TowerRequestConfig {
        ..Default::default()
    };
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

pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
    request_settings: TowerRequestSettings,
    client: CloudWatchLogsClient,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

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

impl CloudwatchLogsSinkConfig {
    fn create_client(&self) -> crate::Result<CloudWatchLogsClient> {
        let region = (&self.region).try_into()?;

        let client = rusoto::client()?;
        let creds = self.auth.build(&region, self.assume_role.clone())?;

        let client = rusoto_core::Client::new_with_encoding(creds, client, self.compression.into());
        Ok(CloudWatchLogsClient::new_with_client(client, region))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    async fn build(
        &self,
        cx: SinkContext,
    ) -> crate::Result<(super::VectorSink, super::Healthcheck)> {
        let batch = BatchSettings::default()
            .bytes(1_048_576)
            .events(10_000)
            .timeout(1)
            .parse_config(self.batch)?;
        let request = self.request.unwrap_with(&REQUEST_DEFAULTS);

        let log_group = self.group_name.clone();
        let log_stream = self.stream_name.clone();

        let client = self.create_client()?;
        let svc = ServiceBuilder::new()
            .concurrency_limit(request.concurrency.unwrap())
            .service(CloudwatchLogsPartitionSvc::new(
                self.clone(),
                client.clone(),
            ));

        let encoding = self.encoding.clone();
        let buffer = PartitionBuffer::new(VecBuffer::new(batch.size));
        let sink = PartitionBatchSink::new(svc, buffer, batch.timeout, cx.acker())
            .sink_map_err(|error| error!(message = "Fatal cloudwatchlogs sink error.", %error))
            .with_flat_map(move |event| {
                stream::iter(partition_encode(event, &encoding, &log_group, &log_stream)).map(Ok)
            });

        let healthcheck = healthcheck(self.clone(), client).boxed();

        Ok((super::VectorSink::Sink(Box::new(sink)), healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn sink_type(&self) -> &'static str {
        "aws_cloudwatch_logs"
    }
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(config: CloudwatchLogsSinkConfig, client: CloudWatchLogsClient) -> Self {
        let request_settings = config.request.unwrap_with(&REQUEST_DEFAULTS);

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
            // Buffer size is `concurrency` because current service always ready.
            // Concurrency limit is 1 because we need token from previous request.
            let svc = ServiceBuilder::new()
                .buffer(self.request_settings.concurrency.unwrap())
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

impl EncodedLength for InputLogEvent {
    fn encoded_length(&self) -> usize {
        self.message.len() + 26
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: String,
    stream: String,
}

fn encode_log(
    mut log: LogEvent,
    encoding: &EncodingConfig<Encoding>,
) -> Result<InputLogEvent, CloudwatchLogsError> {
    let timestamp = match log.remove(log_schema().timestamp_key()) {
        Some(Value::Timestamp(ts)) => ts.timestamp_millis(),
        _ => Utc::now().timestamp_millis(),
    };

    let message = match encoding.codec() {
        Encoding::Json => serde_json::to_string(&log).unwrap(),
        Encoding::Text => log
            .get(log_schema().message_key())
            .map(|v| v.to_string_lossy())
            .unwrap_or_else(|| "".into()),
    };

    match message.len() {
        length if length <= MAX_MESSAGE_SIZE => Ok(InputLogEvent { message, timestamp }),
        length => Err(CloudwatchLogsError::EventTooLong { length }),
    }
}

fn partition_encode(
    mut event: Event,
    encoding: &EncodingConfig<Encoding>,
    group: &Template,
    stream: &Template,
) -> Option<PartitionInnerBuffer<InputLogEvent, CloudwatchKey>> {
    let group = match group.render_string(&event) {
        Ok(b) => b,
        Err(missing_keys) => {
            warn!(
                message = "Keys in group template do not exist on the event; dropping event.",
                ?missing_keys,
                internal_log_rate_secs = 30
            );
            return None;
        }
    };

    let stream = match stream.render_string(&event) {
        Ok(b) => b,
        Err(missing_keys) => {
            warn!(
                message = "Keys in stream template do not exist on the event; dropping event.",
                ?missing_keys,
                internal_log_rate_secs = 30
            );
            return None;
        }
    };

    let key = CloudwatchKey { stream, group };

    encoding.apply_rules(&mut event);
    let event = encode_log(event.into_log(), encoding)
        .map_err(
            |error| error!(message = "Could not encode event.", %error, internal_log_rate_secs = 5),
        )
        .ok()?;

    Some(PartitionInnerBuffer::new(event, key))
}

#[derive(Debug, Snafu)]
enum HealthcheckError {
    #[snafu(display("DescribeLogGroups failed: {}", source))]
    DescribeLogGroupsFailed {
        source: RusotoError<rusoto_logs::DescribeLogGroupsError>,
    },
    #[snafu(display("No log group found"))]
    NoLogGroup,
    #[snafu(display("Unable to extract group name"))]
    GroupNameError,
    #[snafu(display("Group name mismatch: expected {}, found {}", expected, name))]
    GroupNameMismatch { expected: String, name: String },
}

async fn healthcheck(
    config: CloudwatchLogsSinkConfig,
    client: CloudWatchLogsClient,
) -> crate::Result<()> {
    if config.group_name.is_dynamic() {
        info!("Cloudwatch group_name is dynamic; skipping healthcheck.");
        return Ok(());
    }

    let group_name = config.group_name.get_ref().to_owned();
    let expected_group_name = group_name.clone();

    let request = DescribeLogGroupsRequest {
        limit: Some(1),
        log_group_name_prefix: Some(group_name),
        ..Default::default()
    };

    // This will attempt to find the group name passed in and verify that
    // it matches the one that AWS sends back.
    match client.describe_log_groups(request).await {
        Ok(resp) => match resp.log_groups.and_then(|g| g.into_iter().next()) {
            Some(group) => {
                if let Some(name) = group.log_group_name {
                    if name == expected_group_name {
                        Ok(())
                    } else {
                        Err(HealthcheckError::GroupNameMismatch {
                            expected: expected_group_name,
                            name,
                        }
                        .into())
                    }
                } else {
                    Err(HealthcheckError::GroupNameError.into())
                }
            }
            None => Err(HealthcheckError::NoLogGroup.into()),
        },
        Err(source) => Err(HealthcheckError::DescribeLogGroupsFailed { source }.into()),
    }
}

#[derive(Debug, Clone)]
struct CloudwatchRetryLogic;

impl RetryLogic for CloudwatchRetryLogic {
    type Error = CloudwatchError;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => match err {
                RusotoError::Service(PutLogEventsError::ServiceUnavailable(error)) => {
                    error!(message = "Put logs service unavailable.", %error);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Put logs HTTP dispatch.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Put logs HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::Unknown(res)
                    if rusoto_core::proto::json::Error::parse(&res)
                        .filter(|error| error.typ.as_str() == "ThrottlingException")
                        .is_some() =>
                {
                    true
                }

                _ => false,
            },

            CloudwatchError::Describe(err) => match err {
                RusotoError::Service(DescribeLogStreamsError::ServiceUnavailable(error)) => {
                    error!(message = "Describe streams service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Describe streams HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Describe streams HTTP dispatch.", %error);
                    true
                }

                _ => false,
            },

            CloudwatchError::CreateStream(err) => match err {
                RusotoError::Service(CreateLogStreamError::ServiceUnavailable(error)) => {
                    error!(message = "Create stream service unavailable.", %error);
                    true
                }

                RusotoError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "Create stream HTTP error.", status = %status, body = %body);
                    true
                }

                RusotoError::HttpDispatch(error) => {
                    error!(message = "Create stream HTTP dispatch.", %error);
                    true
                }

                _ => false,
            },
            _ => false,
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{Event, Value},
        rusoto::RegionOrEndpoint,
    };
    use std::collections::HashMap;
    use std::convert::{TryFrom, TryInto};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<CloudwatchLogsSinkConfig>();
    }

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Template::try_from("stream").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let (_event, key) = partition_encode(event, &encoding, &group, &stream)
            .unwrap()
            .into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let (_event, key) = partition_encode(event, &encoding, &group, &stream)
            .unwrap()
            .into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_prefix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("abcd-{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let (_event, key) = partition_encode(event, &encoding, &group, &stream)
            .unwrap()
            .into_parts();

        let expected = CloudwatchKey {
            stream: "abcd-stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_postfix() {
        let mut event = Event::from("hello world");

        event.as_mut_log().insert("log_stream", "stream");

        let stream = Template::try_from("{{log_stream}}-abcd").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let (_event, key) = partition_encode(event, &encoding, &group, &stream)
            .unwrap()
            .into_parts();

        let expected = CloudwatchKey {
            stream: "stream-abcd".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");

        let stream = Template::try_from("{{log_stream}}").unwrap();
        let group = "group".try_into().unwrap();
        let encoding = Encoding::Text.into();

        let stream_val = partition_encode(event, &encoding, &group, &stream);

        assert!(stream_val.is_none());
    }

    fn svc(config: CloudwatchLogsSinkConfig) -> CloudwatchLogsSvc {
        let config = CloudwatchLogsSinkConfig {
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..config
        };
        let key = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };
        let client = config.create_client().unwrap();
        CloudwatchLogsSvc::new(&config, &key, client)
    }

    #[test]
    fn cloudwatch_encoded_event_retains_timestamp() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event.clone(), &Encoding::Json.into()).unwrap();

        let ts = if let Value::Timestamp(ts) = event[log_schema().timestamp_key()] {
            ts.timestamp_millis()
        } else {
            panic!()
        };

        assert_eq!(encoded.timestamp, ts);
    }

    #[test]
    fn cloudwatch_encode_log_as_json() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event, &Encoding::Json.into()).unwrap();
        let map: HashMap<String, String> = serde_json::from_str(&encoded.message[..]).unwrap();
        assert!(map.get(log_schema().timestamp_key()).is_none());
    }

    #[test]
    fn cloudwatch_encode_log_as_text() {
        let mut event = Event::from("hello world").into_log();
        event.insert("key", "value");
        let encoded = encode_log(event, &Encoding::Text.into()).unwrap();
        assert_eq!(encoded.message, "hello world");
    }

    #[test]
    fn cloudwatch_24h_split() {
        let now = Utc::now();
        let events = (0..100)
            .map(|i| now - Duration::hours(i))
            .map(|timestamp| {
                let mut event = Event::new_empty_log();
                event
                    .as_mut_log()
                    .insert(log_schema().timestamp_key(), timestamp);
                encode_log(event.into_log(), &Encoding::Text.into()).unwrap()
            })
            .collect();

        let batches = svc(default_config(Encoding::Text)).process_events(events);

        let day = Duration::days(1).num_milliseconds();
        for batch in batches.iter() {
            assert!((batch.last().unwrap().timestamp - batch.first().unwrap().timestamp) <= day);
        }

        assert_eq!(batches.len(), 5);
    }
}

#[cfg(feature = "aws-cloudwatch-logs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::{
        config::{SinkConfig, SinkContext},
        rusoto::RegionOrEndpoint,
        test_util::{random_lines, random_lines_with_stream, random_string, trace_init},
    };
    use futures::{stream, SinkExt, StreamExt};
    use pretty_assertions::assert_eq;
    use rusoto_core::Region;
    use rusoto_logs::{CloudWatchLogs, CreateLogGroupRequest, GetLogEventsRequest};
    use std::convert::TryFrom;

    const GROUP_NAME: &str = "vector-cw";

    #[tokio::test]
    async fn cloudwatch_insert_log_event() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);
        sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_events_sorted() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now() - chrono::Duration::days(1);

        let (mut input_lines, events) = random_lines_with_stream(100, 11);

        // add a historical timestamp to all but the first event, to simulate
        // out-of-order timestamps.
        let mut doit = false;
        let events = events.map(move |mut event| {
            if doit {
                let timestamp = chrono::Utc::now() - chrono::Duration::days(1);

                event
                    .as_mut_log()
                    .insert(log_schema().timestamp_key(), Value::Timestamp(timestamp));
            }
            doit = true;

            event
        });
        let _ = sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        // readjust input_lines in the same way we have readjusted timestamps.
        let first = input_lines.remove(0);
        input_lines.push(first);
        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_out_of_range_timestamp() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let now = chrono::Utc::now();

        let mut input_lines = random_lines(100);
        let mut events = Vec::new();
        let mut lines = Vec::new();

        let mut add_event = |offset: chrono::Duration| {
            let line = input_lines.next().unwrap();
            let mut event = Event::from(line.clone());
            event
                .as_mut_log()
                .insert(log_schema().timestamp_key(), now + offset);
            events.push(event);
            line
        };

        // Too old event (> 14 days)
        add_event(Duration::days(-15));
        // Too new event (> 2 hours)
        add_event(Duration::minutes(125));
        // Right now and future in +2h limit
        lines.push(add_event(Duration::zero()));
        lines.push(add_event(Duration::hours(1)));
        lines.push(add_event(Duration::minutes(110)));
        // In 14 days limit
        lines.push(add_event(Duration::days(-1)));
        lines.push(add_event(Duration::days(-13)));

        sink.run(stream::iter(events)).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: GROUP_NAME.into(),
            start_time: Some((now - Duration::days(30)).timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, lines);
    }

    #[tokio::test]
    async fn cloudwatch_dynamic_group_and_stream_creation() {
        trace_init();

        let stream_name = gen_name();
        let group_name = gen_name();

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(group_name.as_str()).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);
        sink.run(events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: group_name,
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_event_batched() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let group_name = gen_name();

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from(stream_name.as_str()).unwrap(),
            group_name: Template::try_from(group_name.as_str()).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: BatchConfig {
                max_events: Some(2),
                ..Default::default()
            },
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);
        let mut events = events.map(Ok);
        let _ = sink.into_sink().send_all(&mut events).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: stream_name,
            log_group_name: group_name,
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[tokio::test]
    async fn cloudwatch_insert_log_event_partitioned() {
        trace_init();

        ensure_group().await;

        let stream_name = gen_name();
        let config = CloudwatchLogsSinkConfig {
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            stream_name: Template::try_from(format!("{}-{{{{key}}}}", stream_name)).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let (sink, _) = config.build(SinkContext::new_test()).await.unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, _events) = random_lines_with_stream(100, 10);

        let events = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let mut event = Event::from(e);
                let stream = format!("{}", (i % 2));
                event.as_mut_log().insert("key", stream);
                event
            })
            .collect::<Vec<_>>();
        sink.run(stream::iter(events)).await.unwrap();

        let request = GetLogEventsRequest {
            log_stream_name: format!("{}-0", stream_name),
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 0)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);

        let request = GetLogEventsRequest {
            log_stream_name: format!("{}-1", stream_name),
            log_group_name: GROUP_NAME.into(),
            start_time: Some(timestamp.timestamp_millis()),
            ..Default::default()
        };

        let response = create_client_test().get_log_events(request).await.unwrap();
        let events = response.events.unwrap();
        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();
        let expected_output = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .filter(|(i, _)| i % 2 == 1)
            .map(|(_, e)| e)
            .collect::<Vec<_>>();

        assert_eq!(output_lines, expected_output);
    }

    #[tokio::test]
    async fn cloudwatch_healthcheck() {
        trace_init();

        ensure_group().await;

        let config = CloudwatchLogsSinkConfig {
            stream_name: Template::try_from("test-stream").unwrap(),
            group_name: Template::try_from(GROUP_NAME).unwrap(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            encoding: Encoding::Text.into(),
            create_missing_group: None,
            create_missing_stream: None,
            compression: Default::default(),
            batch: Default::default(),
            request: Default::default(),
            assume_role: None,
            auth: Default::default(),
        };

        let client = config.create_client().unwrap();
        healthcheck(config, client).await.unwrap();
    }

    fn create_client_test() -> CloudWatchLogsClient {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        let client = rusoto::client().unwrap();
        let creds = rusoto::AwsCredentialsProvider::new(&region, None).unwrap();
        CloudWatchLogsClient::new_with(client, creds, region)
    }

    async fn ensure_group() {
        let client = create_client_test();
        let req = CreateLogGroupRequest {
            log_group_name: GROUP_NAME.into(),
            ..Default::default()
        };
        let _ = client.create_log_group(req).await;
    }

    fn gen_name() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
