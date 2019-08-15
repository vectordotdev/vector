mod request;

use crate::{
    buffers::Acker,
    event::{self, Event, LogEvent, ValueKind},
    region::RegionOrEndpoint,
    sinks::util::{
        retries::{FixedRetryPolicy, RetryLogic},
        BatchServiceSink, PartitionBuffer, PartitionInnerBuffer, SinkExt,
    },
    template::Template,
    topology::config::{DataType, SinkConfig},
};
use bytes::Bytes;
use futures::{stream::iter_ok, sync::oneshot, Async, Future, Poll, Sink};
use rusoto_core::{
    request::{BufferedHttpResponse, HttpClient},
    Region,
};
use rusoto_credential::DefaultCredentialsProvider;
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogStreamError, DescribeLogGroupsRequest,
    DescribeLogStreamsError, InputLogEvent, PutLogEventsError,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto, fmt, time::Duration};
use tower::{
    buffer::Buffer,
    limit::{
        concurrency::ConcurrencyLimit,
        rate::{Rate, RateLimit},
    },
    retry::Retry,
    timeout::Timeout,
    Service, ServiceBuilder, ServiceExt,
};

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct CloudwatchLogsSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    #[serde(flatten)]
    pub region: RegionOrEndpoint,
    pub batch_timeout: Option<u64>,
    pub batch_size: Option<usize>,
    pub encoding: Option<Encoding>,

    // Tower Request based configuration
    pub request_in_flight_limit: Option<usize>,
    pub request_timeout_secs: Option<u64>,
    pub request_rate_limit_duration_secs: Option<u64>,
    pub request_rate_limit_num: Option<u64>,
    pub request_retry_attempts: Option<usize>,
    pub request_retry_backoff_secs: Option<u64>,
}

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    encoding: Option<Encoding>,
    stream_name: String,
    group_name: String,
    token: Option<String>,
    token_rx: Option<oneshot::Receiver<Option<String>>>,
}

type Svc = Buffer<
    ConcurrencyLimit<
        RateLimit<
            Retry<
                FixedRetryPolicy<CloudwatchRetryLogic>,
                Buffer<Timeout<CloudwatchLogsSvc>, Vec<Event>>,
            >,
        >,
    >,
    Vec<Event>,
>;

pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
    request_config: RequestConfig,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Debug, Copy, Clone)]
pub struct RequestConfig {
    timeout_secs: u64,
    rate_limit_duration_secs: u64,
    rate_limit_num: u64,
    retry_attempts: usize,
    retry_backoff_secs: u64,
}

#[derive(Debug)]
pub enum CloudwatchError {
    Put(PutLogEventsError),
    Describe(DescribeLogStreamsError),
    CreateStream(CreateLogStreamError),
    NoStreamsFound,
    ServiceDropped,
    MakeService,
}

#[typetag::serde(name = "aws_cloudwatch_logs")]
impl SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let batch_timeout = self.batch_timeout.unwrap_or(1);
        let batch_size = self.batch_size.unwrap_or(1000);

        let log_group = self.group_name.clone().into();
        let log_stream = Template::from(self.stream_name.as_str());

        let in_flight_limit = self.request_in_flight_limit.unwrap_or(5);

        let svc = ServiceBuilder::new()
            .concurrency_limit(in_flight_limit)
            .service(CloudwatchLogsPartitionSvc::new(self.clone())?);

        let sink = {
            let svc_sink = BatchServiceSink::new(svc, acker)
                .partitioned_batched_with_min(
                    PartitionBuffer::new(Vec::new()),
                    batch_size,
                    Duration::from_secs(batch_timeout),
                )
                .with_flat_map(move |event| iter_ok(partition(event, &log_group, &log_stream)));
            Box::new(svc_sink)
        };

        let healthcheck = healthcheck(self.clone())?;

        Ok((sink, healthcheck))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(config: CloudwatchLogsSinkConfig) -> Result<Self, String> {
        let timeout_secs = config.request_timeout_secs.unwrap_or(60);
        let rate_limit_duration_secs = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);
        let retry_attempts = config.request_retry_attempts.unwrap_or(usize::max_value());
        let retry_backoff_secs = config.request_retry_backoff_secs.unwrap_or(1);

        let request_config = RequestConfig {
            timeout_secs,
            rate_limit_duration_secs,
            rate_limit_num,
            retry_attempts,
            retry_backoff_secs,
        };

        Ok(Self {
            config,
            clients: HashMap::new(),
            request_config,
        })
    }
}

impl Service<PartitionInnerBuffer<Vec<Event>, CloudwatchKey>> for CloudwatchLogsPartitionSvc {
    type Response = ();
    type Error = Box<std::error::Error + Send + Sync + 'static>;
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        Ok(().into())
    }

    fn call(&mut self, req: PartitionInnerBuffer<Vec<Event>, CloudwatchKey>) -> Self::Future {
        let (events, key) = req.into_parts();

        let RequestConfig {
            timeout_secs,
            rate_limit_duration_secs,
            rate_limit_num,
            retry_attempts,
            retry_backoff_secs,
        } = self.request_config;

        let svc = if let Some(svc) = &mut self.clients.get_mut(&key) {
            svc.clone()
        } else {
            let svc = {
                let policy = FixedRetryPolicy::new(
                    retry_attempts,
                    Duration::from_secs(retry_backoff_secs),
                    CloudwatchRetryLogic,
                );

                let cloudwatch = CloudwatchLogsSvc::new(&self.config, &key).unwrap();
                let timeout = Timeout::new(cloudwatch, Duration::from_secs(timeout_secs));

                let buffer = Buffer::new(timeout, 1);
                let retry = Retry::new(policy, buffer);

                let rate = RateLimit::new(
                    retry,
                    Rate::new(
                        rate_limit_num,
                        Duration::from_secs(rate_limit_duration_secs),
                    ),
                );
                let concurrency = ConcurrencyLimit::new(rate, 1);

                Buffer::new(concurrency, 1)
            };

            self.clients.insert(key, svc.clone());
            svc
        };

        let fut = svc
            .ready()
            .map_err(Into::into)
            .and_then(move |mut svc| svc.call(events))
            .map_err(Into::into);

        Box::new(fut)
    }
}

impl CloudwatchLogsSvc {
    pub fn new(config: &CloudwatchLogsSinkConfig, key: &CloudwatchKey) -> Result<Self, String> {
        let region = config.region.clone().try_into()?;
        let client = create_client(region)?;

        let group_name = String::from_utf8_lossy(&key.group[..]).into_owned();
        let stream_name = String::from_utf8_lossy(&key.stream[..]).into_owned();

        Ok(CloudwatchLogsSvc {
            client,
            encoding: config.encoding.clone(),
            stream_name,
            group_name,
            token: None,
            token_rx: None,
        })
    }

    pub fn encode_log(&self, mut log: LogEvent) -> InputLogEvent {
        let timestamp = if let Some(ValueKind::Timestamp(ts)) = log.remove(&event::TIMESTAMP) {
            ts.timestamp_millis()
        } else {
            chrono::Utc::now().timestamp_millis()
        };

        match (&self.encoding, log.is_structured()) {
            (&Some(Encoding::Json), _) | (_, true) => {
                let bytes = serde_json::to_vec(&log.unflatten()).unwrap();
                let message = String::from_utf8(bytes).unwrap();

                InputLogEvent { message, timestamp }
            }
            (&Some(Encoding::Text), _) | (_, false) => {
                let message = log
                    .get(&event::MESSAGE)
                    .map(|v| v.to_string_lossy())
                    .unwrap_or_else(|| "".into());
                InputLogEvent { message, timestamp }
            }
        }
    }
}

impl Service<Vec<Event>> for CloudwatchLogsSvc {
    type Response = ();
    type Error = CloudwatchError;
    type Future = request::CloudwatchFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Some(rx) = &mut self.token_rx {
            match rx.poll() {
                Ok(Async::Ready(token)) => {
                    self.token = token;
                    self.token_rx = None;
                    Ok(().into())
                }
                Ok(Async::NotReady) => Ok(Async::NotReady),
                Err(oneshot::Canceled) => {
                    // This case only happens when the `tx` end gets dropped due to an error
                    // in this case we just reset the token and try again.
                    self.token = None;
                    self.token_rx = None;
                    Ok(().into())
                }
            }
        } else {
            Ok(().into())
        }
    }

    fn call(&mut self, req: Vec<Event>) -> Self::Future {
        if self.token_rx.is_none() {
            let events = req
                .into_iter()
                .map(|e| e.into_log())
                .map(|e| self.encode_log(e))
                .collect::<Vec<_>>();

            let (tx, rx) = oneshot::channel();
            self.token_rx = Some(rx);

            debug!(message = "Sending events.", events = %events.len());
            request::CloudwatchFuture::new(
                self.client.clone(),
                self.stream_name.clone(),
                self.group_name.clone(),
                events,
                self.token.take(),
                tx,
            )
        } else {
            panic!("poll_ready was not called; this is a bug!");
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: Bytes,
    stream: Bytes,
}

fn partition(
    event: Event,
    group: &Bytes,
    stream: &Template,
) -> Option<PartitionInnerBuffer<Event, CloudwatchKey>> {
    let stream = match stream.render(&event) {
        Ok(b) => b,
        Err(missing_keys) => {
            warn!(
                message = "Keys do not exist on the event. Dropping event.",
                keys = ?missing_keys
            );
            return None;
        }
    };

    let key = CloudwatchKey {
        stream,
        group: group.clone(),
    };

    Some(PartitionInnerBuffer::new(event, key))
}

fn healthcheck(config: CloudwatchLogsSinkConfig) -> Result<super::Healthcheck, String> {
    let region = config.region.clone();

    let client = create_client(region.try_into()?)?;

    let request = DescribeLogGroupsRequest {
        limit: Some(1),
        log_group_name_prefix: config.group_name.clone().into(),
        ..Default::default()
    };

    let expected_group_name = config.group_name.clone();

    // This will attempt to find the group name passed in and verify that
    // it matches the one that AWS sends back.
    let fut = client
        .describe_log_groups(request)
        .map_err(|e| format!("DescribeLogStreams failed: {}", e))
        .and_then(|response| {
            response
                .log_groups
                .ok_or_else(|| "No log group found".to_string())
        })
        .and_then(move |groups| {
            if let Some(group) = groups.into_iter().next() {
                if let Some(name) = group.log_group_name {
                    if name == expected_group_name {
                        Ok(())
                    } else {
                        Err(format!(
                            "Group name mismatch: Expected {}, found {}",
                            expected_group_name, name
                        ))
                    }
                } else {
                    Err("Unable to extract group name".to_string())
                }
            } else {
                Err("No log group found".to_string())
            }
        });

    Ok(Box::new(fut))
}

fn create_client(region: Region) -> Result<CloudWatchLogsClient, String> {
    let http = HttpClient::new().map_err(|e| format!("{}", e))?;
    let creds = DefaultCredentialsProvider::new().map_err(|e| format!("{}", e))?;

    Ok(CloudWatchLogsClient::new_with(http, creds, region))
}

#[derive(Debug, Clone)]
struct CloudwatchRetryLogic;

impl RetryLogic for CloudwatchRetryLogic {
    type Error = CloudwatchError;
    type Response = ();

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        match error {
            CloudwatchError::Put(err) => match err {
                PutLogEventsError::ServiceUnavailable(error) => {
                    error!(message = "put logs service unavailable.", %error);
                    true
                }

                PutLogEventsError::HttpDispatch(error) => {
                    error!(message = "put logs http dispatch.", %error);
                    true
                }

                PutLogEventsError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "put logs http error.", %status, %body);
                    true
                }

                _ => false,
            },

            CloudwatchError::Describe(err) => match err {
                DescribeLogStreamsError::ServiceUnavailable(error) => {
                    error!(message = "describe streams service unavailable.", %error);
                    true
                }

                DescribeLogStreamsError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "describe streams http error.", %status, %body);
                    true
                }

                DescribeLogStreamsError::HttpDispatch(error) => {
                    error!(message = "describe streams http dispatch.", %error);
                    true
                }

                _ => false,
            },

            CloudwatchError::CreateStream(err) => match err {
                CreateLogStreamError::ServiceUnavailable(error) => {
                    error!(message = "create stream service unavailable.", %error);
                    true
                }

                CreateLogStreamError::Unknown(res)
                    if res.status.is_server_error()
                        || res.status == http::StatusCode::TOO_MANY_REQUESTS =>
                {
                    let BufferedHttpResponse { status, body, .. } = res;
                    let body = String::from_utf8_lossy(&body[..]);
                    let body = &body[..body.len().min(50)];

                    error!(message = "create stream http error.", %status, %body);
                    true
                }

                CreateLogStreamError::HttpDispatch(error) => {
                    error!(message = "create stream http dispatch.", %error);
                    true
                }

                _ => false,
            },
            _ => false,
        }
    }
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CloudwatchError::Put(e) => write!(f, "CloudwatchError::Put: {}", e),
            CloudwatchError::Describe(e) => write!(f, "CloudwatchError::Describe: {}", e),
            CloudwatchError::CreateStream(e) => write!(f, "CloudwatchError::CreateStream: {}", e),
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

impl From<PutLogEventsError> for CloudwatchError {
    fn from(e: PutLogEventsError) -> Self {
        CloudwatchError::Put(e)
    }
}

impl From<DescribeLogStreamsError> for CloudwatchError {
    fn from(e: DescribeLogStreamsError) -> Self {
        CloudwatchError::Describe(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{self, Event, ValueKind},
        region::RegionOrEndpoint,
    };
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Template::from("stream");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event() {
        let mut event = Event::from("hello world");

        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());

        let stream = Template::from("{{log_stream}}");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_prefix() {
        let mut event = Event::from("hello world");

        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());

        let stream = Template::from("abcd-{{log_stream}}");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "abcd-stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_event_with_postfix() {
        let mut event = Event::from("hello world");

        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());

        let stream = Template::from("{{log_stream}}-abcd");
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream).unwrap().into_parts();

        let expected = CloudwatchKey {
            stream: "stream-abcd".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");

        let stream = Template::from("{{log_stream}}");
        let group = "group".into();

        let stream_val = partition(event, &group, &stream);

        assert!(stream_val.is_none());
    }

    #[test]
    fn cloudwatch_encode_log() {
        let config = CloudwatchLogsSinkConfig {
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };
        let key = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        let svc = CloudwatchLogsSvc::new(&config, &key).unwrap();

        let mut event = Event::from("hello world").into_log();

        event.insert_explicit("key".into(), "value".into());

        let input_event = svc.encode_log(event.clone());

        let ts = if let ValueKind::Timestamp(ts) = event[&event::TIMESTAMP] {
            ts.timestamp_millis()
        } else {
            panic!()
        };

        assert_eq!(input_event.timestamp, ts);

        let bytes = input_event.message;

        let map: HashMap<Atom, String> = serde_json::from_str(&bytes[..]).unwrap();

        assert!(map.get(&event::TIMESTAMP).is_none());
    }
}

#[cfg(feature = "cloudwatch-logs-integration-tests")]
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::buffers::Acker;
    use crate::{
        region::RegionOrEndpoint,
        test_util::{block_on, random_lines_with_stream, random_string},
        topology::config::SinkConfig,
    };
    use futures::Sink;
    use pretty_assertions::assert_eq;
    use rusoto_core::Region;
    use rusoto_logs::{CloudWatchLogs, CreateLogGroupRequest, GetLogEventsRequest};
    use tokio::runtime::current_thread::Runtime;

    const GROUP_NAME: &'static str = "vector-cw";

    #[test]
    fn cloudwatch_insert_log_event() {
        let mut rt = Runtime::new().unwrap();

        let stream_name = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: stream_name.clone().into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };

        let (sink, _) = config.build(Acker::Null).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = stream_name.clone().into();
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let client = create_client(region).unwrap();

        let response = rt.block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_insert_log_event_batched() {
        let mut rt = Runtime::new().unwrap();

        let stream_name = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: stream_name.clone().into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            batch_size: Some(2),
            ..Default::default()
        };

        let (sink, _) = config.build(Acker::Null).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = stream_name.clone().into();
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let client = create_client(region).unwrap();

        let response = rt.block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_insert_log_event_partitioned() {
        let mut rt = Runtime::new().unwrap();

        let stream_name = gen_stream();

        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        let client = create_client(region.clone()).unwrap();
        ensure_group(region);

        let config = CloudwatchLogsSinkConfig {
            group_name: GROUP_NAME.into(),
            stream_name: format!("{}-{{{{key}}}}", stream_name).into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };

        let (sink, _) = config.build(Acker::Null).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, _) = random_lines_with_stream(100, 10);

        let events = input_lines
            .clone()
            .into_iter()
            .enumerate()
            .map(|(i, e)| {
                let mut event = Event::from(e);
                let stream = format!("{}", (i % 2));
                event
                    .as_mut_log()
                    .insert_implicit("key".into(), stream.into());
                event
            })
            .collect::<Vec<_>>();

        let pump = sink.send_all(iter_ok(events));
        let (sink, _) = rt.block_on(pump).unwrap();
        // drop the sink so it closes all its connections
        drop(sink);

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = format!("{}-0", stream_name);
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let response = rt.block_on(client.get_log_events(request)).unwrap();
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

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = format!("{}-1", stream_name);
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        let response = rt.block_on(client.get_log_events(request)).unwrap();
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

    #[test]
    fn cloudwatch_healthcheck() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region);

        let config = CloudwatchLogsSinkConfig {
            stream_name: "test-stream".into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };

        block_on(healthcheck(config).unwrap()).unwrap();
    }

    fn ensure_group(region: Region) {
        let client = create_client(region).unwrap();

        let req = CreateLogGroupRequest {
            log_group_name: GROUP_NAME.into(),
            ..Default::default()
        };

        match client.create_log_group(req).sync() {
            Ok(_) => (),
            Err(_) => (),
        };
    }

    fn gen_stream() -> String {
        format!("test-{}", random_string(10).to_lowercase())
    }
}
