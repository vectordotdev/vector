use crate::buffers::Acker;
use crate::{
    event::{self, Event, LogEvent, ValueKind},
    region::RegionOrEndpoint,
    sinks::util::{BatchServiceSink, PartitionBuffer, PartitionInnerBuffer, SinkExt},
};
use bytes::Bytes;
use futures::{stream::iter_ok, sync::oneshot, try_ready, Async, Future, Poll, Sink};
use rusoto_core::RusotoFuture;
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogStreamError, CreateLogStreamRequest,
    DescribeLogGroupsRequest, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fmt;
use std::time::Duration;
use string_cache::DefaultAtom as Atom;
use tokio_trace::field;
use tower::{
    buffer::Buffer,
    limit::{
        concurrency::ConcurrencyLimit,
        rate::{Rate, RateLimit},
    },
    timeout::Timeout,
    Service, ServiceExt,
};

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    state: State,
    encoding: Option<Encoding>,
    stream_name: String,
    group_name: String,
}

#[derive(Debug, Clone, PartialEq)]
enum Partition {
    Static(Bytes),
    Event(Atom),
}

type Svc = Buffer<ConcurrencyLimit<RateLimit<Timeout<CloudwatchLogsSvc>>>, Vec<Event>>;

pub struct CloudwatchLogsPartitionSvc {
    config: CloudwatchLogsSinkConfig,
    clients: HashMap<CloudwatchKey, Svc>,
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
impl crate::topology::config::SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self, acker: Acker) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let svc = CloudwatchLogsPartitionSvc::new(self.clone())?;

        let batch_timeout = self.batch_timeout.unwrap_or(1);
        let batch_size = self.batch_size.unwrap_or(1000);

        let log_group = self.group_name.clone().into();
        let log_stream = interpolate(&self.stream_name);

        let sink = {
            let svc_sink = BatchServiceSink::new(svc, acker)
                .partitioned_batched_with_min(
                    PartitionBuffer::new(Vec::new()),
                    batch_size,
                    Duration::from_secs(batch_timeout),
                )
                .with_flat_map(move |event| partition(event, &log_group, &log_stream));
            Box::new(svc_sink)
        };

        let healthcheck = healthcheck(self.clone())?;

        Ok((sink, healthcheck))
    }
}

impl CloudwatchLogsPartitionSvc {
    pub fn new(config: CloudwatchLogsSinkConfig) -> Result<Self, String> {
        Ok(Self {
            config,
            clients: HashMap::new(),
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

        let timeout = self.config.request_timeout_secs.unwrap_or(60);
        let in_flight_limit = self.config.request_in_flight_limit.unwrap_or(5);
        let rate_limit_duration = self.config.request_rate_limit_duration_secs.unwrap_or(1);
        let rate_limit_num = self.config.request_rate_limit_num.unwrap_or(5);

        let svc = if let Some(svc) = &mut self.clients.get_mut(&key) {
            svc.clone()
        } else {
            let svc = {
                let cloudwatch = CloudwatchLogsSvc::new(&self.config).unwrap();
                let timeout = Timeout::new(cloudwatch, Duration::from_secs(timeout));

                // TODO: add Buffer/Retry here

                let rate = RateLimit::new(
                    timeout,
                    Rate::new(rate_limit_num, Duration::from_secs(rate_limit_duration)),
                );
                let concurrency = ConcurrencyLimit::new(rate, in_flight_limit);

                Buffer::new(concurrency, 5)
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
    pub fn new(config: &CloudwatchLogsSinkConfig) -> Result<Self, String> {
        let region = config.region.clone().try_into()?;
        let client = CloudWatchLogsClient::new(region);

        Ok(CloudwatchLogsSvc {
            client,
            encoding: config.encoding.clone(),
            stream_name: config.stream_name.clone(),
            group_name: config.group_name.clone(),
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

                debug!(message = "Submitting events.", amount_of_events = req.len());
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
struct CloudwatchKey {
    group: Bytes,
    stream: Bytes,
}

fn interpolate(s: &str) -> Partition {
    use regex::Regex;
    let r = Regex::new(r"\{event\.(?P<key>\D+)\}").unwrap();

    if let Some(cap) = r.captures(s) {
        if let Some(m) = cap.name("key") {
            return Partition::Event(m.as_str().into());
        }
    }

    Partition::Static(s.into())
}

fn partition(
    event: Event,
    group: &Bytes,
    stream: &Partition,
) -> impl futures::Stream<Item = PartitionInnerBuffer<Event, CloudwatchKey>, Error = ()> {
    let stream = match stream {
        Partition::Static(g) => g.clone(),
        Partition::Event(key) => {
            if let Some(val) = event.as_log().get(&key) {
                val.as_bytes().clone()
            } else {
                warn!(
                    message =
                        "Event key does not exist on the event and the event will be dropped.",
                    key = field::debug(key)
                );
                return iter_ok(vec![]);
            }
        }
    };

    let key = CloudwatchKey {
        stream,
        group: group.clone(),
    };

    iter_ok(vec![PartitionInnerBuffer::new(event, key)])
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
    use futures::Stream;
    use std::collections::HashMap;
    use string_cache::DefaultAtom as Atom;

    #[test]
    fn interpolate_event() {
        let partition = interpolate("{event.some_key}");

        assert_eq!(partition, Partition::Event("some_key".into()));
    }

    #[test]
    fn interpolate_static() {
        let partition = interpolate("static_key");

        assert_eq!(partition, Partition::Static("static_key".into()));
    }

    #[test]
    fn partition_static() {
        let event = Event::from("hello world");
        let stream = Partition::Static("stream".into());
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream)
            .wait()
            .into_iter()
            .next()
            .unwrap()
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

        event
            .as_mut_log()
            .insert_implicit("log_stream".into(), "stream".into());

        let stream = Partition::Event("log_stream".into());
        let group = "group".into();

        let (_event, key) = partition(event, &group, &stream)
            .wait()
            .into_iter()
            .next()
            .unwrap()
            .unwrap()
            .into_parts();

        let expected = CloudwatchKey {
            stream: "stream".into(),
            group: "group".into(),
        };

        assert_eq!(key, expected)
    }

    #[test]
    fn partition_no_key_event() {
        let event = Event::from("hello world");

        let stream = Partition::Event("log_stream".into());
        let group = "group".into();

        let stream_val = partition(event, &group, &stream).wait().into_iter().next();

        assert!(stream_val.is_none());
    }

    #[test]
    fn cloudwatch_encode_log() {
        let config = CloudwatchLogsSinkConfig {
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };
        let svc = CloudwatchLogsSvc::new(&config).unwrap();

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
        test_util::{block_on, random_lines_with_stream},
        topology::config::SinkConfig,
    };
    use futures::Sink;
    use rusoto_core::Region;
    use rusoto_logs::{
        CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupRequest, GetLogEventsRequest,
    };

    const STREAM_NAME: &'static str = "test-1";
    const GROUP_NAME: &'static str = "router";

    #[test]
    fn cloudwatch_insert_log_event() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: STREAM_NAME.into(),
            group_name: GROUP_NAME.into(),
            region: RegionOrEndpoint::with_endpoint("http://localhost:6000".into()),
            ..Default::default()
        };

        let (sink, _) = config.build(Acker::Null).unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, events) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(events);
        block_on(pump).unwrap();

        let mut request = GetLogEventsRequest::default();
        request.log_stream_name = STREAM_NAME.into();
        request.log_group_name = GROUP_NAME.into();
        request.start_time = Some(timestamp.timestamp_millis());

        std::thread::sleep(std::time::Duration::from_millis(1000));

        let client = CloudWatchLogsClient::new(region);

        let response = block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

    #[test]
    fn cloudwatch_healthcheck() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };
        ensure_group(region);

        let config = CloudwatchLogsSinkConfig {
            stream_name: STREAM_NAME.into(),
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

}
