use crate::{
    buffers::Acker,
    event::{self, Event, LogEvent, ValueKind},
    region::RegionOrEndpoint,
    sinks::util::{BatchServiceSink, PartitionBuffer, PartitionInnerBuffer, SinkExt},
    topology::config::{DataType, SinkConfig},
};
use bytes::Bytes;
use futures::{stream::iter_ok, sync::oneshot, try_ready, Async, Future, Poll, Sink};
use regex::bytes::{Captures, Regex};
use rusoto_core::RusotoFuture;
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogStreamError, CreateLogStreamRequest,
    DescribeLogGroupsRequest, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto, fmt, time::Duration};
use string_cache::DefaultAtom as Atom;
use tower::{
    buffer::Buffer,
    limit::{
        concurrency::ConcurrencyLimit,
        rate::{Rate, RateLimit},
    },
    timeout::Timeout,
    Service, ServiceExt,
};
use tracing::field;

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    state: State,
    encoding: Option<Encoding>,
    stream_name: String,
    group_name: String,
}

#[derive(Debug, Clone)]
enum Partition {
    /// A static field that doesn't create dynamic partitions
    Static(Bytes),
    /// Represents the ability to extract a key/value from the event
    /// via the provided interpolated stream name.
    Field(Regex, Bytes, Atom),
}

type Svc = Buffer<ConcurrencyLimit<RateLimit<Timeout<CloudwatchLogsSvc>>>, Vec<Event>>;

pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
    request_config: RequestConfig,
}

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
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    Text,
    Json,
}

#[derive(Debug, Copy, Clone)]
pub struct RequestConfig {
    in_flight_limit: usize,
    timeout_secs: u64,
    rate_limit_duration_secs: u64,
    rate_limit_num: u64,
}

enum State {
    Idle,
    Token(Option<String>),
    CreateStream(RusotoFuture<(), CreateLogStreamError>),
    Describe(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(oneshot::Receiver<PutLogEventsResponse>),
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
        let log_stream = interpolate(&self.stream_name);

        let svc = CloudwatchLogsPartitionSvc::new(self.clone())?;

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
        let in_flight_limit = config.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration_secs = config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = config.request_rate_limit_num.unwrap_or(5);

        let request_config = RequestConfig {
            in_flight_limit,
            timeout_secs,
            rate_limit_duration_secs,
            rate_limit_num,
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
            in_flight_limit,
            rate_limit_duration_secs,
            rate_limit_num,
        } = self.request_config;

        let svc = if let Some(svc) = &mut self.clients.get_mut(&key) {
            svc.clone()
        } else {
            let svc = {
                let cloudwatch = CloudwatchLogsSvc::new(&self.config, &key).unwrap();
                let timeout = Timeout::new(cloudwatch, Duration::from_secs(timeout_secs));

                // TODO: add Buffer/Retry here

                let rate = RateLimit::new(
                    timeout,
                    Rate::new(
                        rate_limit_num,
                        Duration::from_secs(rate_limit_duration_secs),
                    ),
                );
                let concurrency = ConcurrencyLimit::new(rate, in_flight_limit);

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
        let client = CloudWatchLogsClient::new(region);

        let group_name = String::from_utf8_lossy(&key.group[..]).into_owned();
        let stream_name = String::from_utf8_lossy(&key.stream[..]).into_owned();

        Ok(CloudwatchLogsSvc {
            client,
            encoding: config.encoding.clone(),
            stream_name,
            group_name,
            state: State::Idle,
        })
    }

    fn put_logs(
        &mut self,
        sequence_token: Option<String>,
        events: Vec<Event>,
    ) -> RusotoFuture<PutLogEventsResponse, PutLogEventsError> {
        let log_events = events
            .into_iter()
            .map(Event::into_log)
            .map(|e| self.encode_log(e))
            .collect();

        let request = PutLogEventsRequest {
            log_events,
            sequence_token,
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        self.client.put_log_events(request)
    }

    fn describe_stream(
        &mut self,
    ) -> RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError> {
        let request = DescribeLogStreamsRequest {
            limit: Some(1),
            log_group_name: self.group_name.clone(),
            log_stream_name_prefix: Some(self.stream_name.clone()),
            ..Default::default()
        };

        self.client.describe_log_streams(request)
    }

    fn create_log_stream(&mut self) -> RusotoFuture<(), CreateLogStreamError> {
        let request = CreateLogStreamRequest {
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        self.client.create_log_stream(request)
    }

    pub fn encode_log(&self, mut log: LogEvent) -> InputLogEvent {
        let timestamp = if let Some(ValueKind::Timestamp(ts)) = log.remove(&event::TIMESTAMP) {
            ts.timestamp_millis()
        } else {
            chrono::Utc::now().timestamp_millis()
        };

        match (&self.encoding, log.is_structured()) {
            (&Some(Encoding::Json), _) | (_, true) => {
                let bytes = serde_json::to_vec(&log.all_fields()).unwrap();
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
    type Future = Box<dyn Future<Item = Self::Response, Error = Self::Error> + Send + 'static>;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        loop {
            match &mut self.state {
                State::Idle => {
                    let fut = self.describe_stream();
                    self.state = State::Describe(fut);
                    continue;
                }
                State::Describe(fut) => {
                    let response = try_ready!(fut.poll().map_err(CloudwatchError::Describe));

                    let stream = if let Some(stream) = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                    {
                        stream
                    } else {
                        let fut = self.create_log_stream();
                        self.state = State::CreateStream(fut);
                        continue;
                    };

                    self.state = State::Token(stream.upload_sequence_token);
                    return Ok(Async::Ready(()));
                }
                State::CreateStream(fut) => {
                    let _ = try_ready!(fut.poll().map_err(CloudwatchError::CreateStream));
                    self.state = State::Idle;
                    continue;
                }
                State::Token(_) => return Ok(Async::Ready(())),
                State::Put(fut) => {
                    let response = match fut.poll() {
                        Ok(Async::Ready(response)) => response,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(_) => panic!("The in flight future was dropped!"),
                    };

                    self.state = State::Token(response.next_sequence_token);
                    return Ok(Async::Ready(()));
                }
            }
        }
    }

    fn call(&mut self, req: Vec<Event>) -> Self::Future {
        match &mut self.state {
            State::Token(token) => {
                let token = token.take();
                let (tx, rx) = oneshot::channel();
                self.state = State::Put(rx);

                debug!(message = "Submitting events.", count = req.len());
                let fut = self
                    .put_logs(token, req)
                    .map_err(CloudwatchError::Put)
                    .and_then(move |res| tx.send(res).map_err(|_| CloudwatchError::ServiceDropped));

                Box::new(fut)
            }
            _ => panic!("You did not call poll_ready!"),
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CloudwatchKey {
    group: Bytes,
    stream: Bytes,
}

fn interpolate(s: &str) -> Partition {
    let r = Regex::new(r"\{\{(?P<key>\D+)\}\}").unwrap();

    if let Some(cap) = r.captures(s.as_bytes()) {
        if let Some(m) = cap.name("key") {
            // TODO(lucio): clean up unwrap
            let key = String::from_utf8(Vec::from(m.as_bytes())).unwrap();
            return Partition::Field(r, s.into(), key.into());
        }
    }

    Partition::Static(s.into())
}

fn partition(
    event: Event,
    group: &Bytes,
    stream: &Partition,
) -> Option<PartitionInnerBuffer<Event, CloudwatchKey>> {
    let stream = match stream {
        Partition::Static(g) => g.clone(),
        Partition::Field(regex, stream, key) => {
            if let Some(val) = event.as_log().get(&key) {
                let cap = regex.replace(stream, |_cap: &Captures| val.as_bytes().clone());
                Bytes::from(&cap[..])
            } else {
                warn!(
                    message =
                        "Event key does not exist on the event and the event will be dropped.",
                    key = field::debug(key)
                );
                return None;
            }
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

    let client = CloudWatchLogsClient::new(region.try_into()?);

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
    fn interpolate_event() {
        if let Partition::Field(_, _, key) = interpolate("{{some_key}}") {
            assert_eq!(key, "some_key".to_string());
        } else {
            panic!("Expected Partition::Field");
        }
    }

    #[test]
    fn interpolate_static() {
        if let Partition::Static(key) = interpolate("static_key") {
            assert_eq!(key, "static_key".to_string());
        } else {
            panic!("Expected Partition::Static");
        }
    }

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Partition::Static("stream".into());
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

        let stream = interpolate("{{log_stream}}");
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

        let stream = interpolate("abcd-{{log_stream}}");
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

        let stream = interpolate("{{log_stream}}-abcd");
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

        let stream = interpolate("{{log_stream}}");
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

#[cfg(feature = "cloudwatch-integration-tests")]
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
    use rusoto_core::Region;
    use rusoto_logs::{
        CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupRequest, GetLogEventsRequest,
    };
    use tokio::runtime::current_thread::Runtime;

    const GROUP_NAME: &'static str = "vector-cw";

    // This test includes both the single partition test
    // and the partitioned test. The reason that we must include
    // both in here is due to the fact that `rusoto` uses a shared
    // client internally. This shared client with lazily create a
    // background task to actually dispatch all the requests. This background
    // task is spawned onto the current executor at time of the request. This is
    // a `hyper` client. Since, `rusoto` uses a shared client when the second test
    // starts roughly at the same time as the other it will use this shared client.
    // If the first test is not done yet and the runtime that was created for that first
    // test has not been dropped, the second test will be able to submit a request. If
    // the first test has _finished_ and the runtime has been _dropped_ then the background
    // task is gone. This will then cause the hyper client in the second test to be unable to
    // send the request down the channel to the background task because its now gone. We combine
    // both tests to ensure that we can use the same runtime and it will only get dropped after both
    // tests have run.
    #[test]
    fn cloudwatch_insert_log_event_and_partitioned() {
        let mut rt = Runtime::new().unwrap();

        // Run single partition test
        {
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

            let client = CloudWatchLogsClient::new(region);

            let response = rt.block_on(client.get_log_events(request)).unwrap();

            let events = response.events.unwrap();

            let output_lines = events
                .into_iter()
                .map(|e| e.message.unwrap())
                .collect::<Vec<_>>();

            assert_eq!(output_lines, input_lines);
        }

        // Run multi partition test
        {
            let stream_name = gen_stream();

            let region = Region::Custom {
                name: "localstack".into(),
                endpoint: "http://localhost:6000".into(),
            };

            let client = CloudWatchLogsClient::new(region.clone());
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
        let client = CloudWatchLogsClient::new(region);

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
