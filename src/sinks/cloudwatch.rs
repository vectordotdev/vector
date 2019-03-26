use crate::{
    record::Record,
    sinks::util::{ServiceSink, SinkExt},
};
use futures::{sync::oneshot, try_ready, Async, Future, Poll};
use rusoto_core::{region::ParseRegionError, Region, RusotoFuture};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};
use std::error::Error as _;
use std::fmt;
use std::time::Duration;
use tower::{Service, ServiceBuilder};
use tower_timeout::TimeoutLayer;

pub struct CloudwatchLogsSvc {
    client: CloudWatchLogsClient,
    state: State,
    config: CloudwatchLogsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CloudwatchLogsSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    pub region: Region,
    pub buffer_size: usize,
}

enum State {
    Idle,
    Token(Option<String>),
    Describe(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(oneshot::Receiver<PutLogEventsResponse>),
}

#[derive(Debug)]
pub enum CloudwatchError {
    Put(PutLogEventsError),
    Describe(DescribeLogStreamsError),
    NoStreamsFound,
    ServiceDropped,
}

#[typetag::serde(name = "cloudwatch_logs")]
impl crate::topology::config::SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let cloudwatch =
            CloudwatchLogsSvc::new(self.clone()).map_err(|e| e.description().to_string())?;

        let svc = ServiceBuilder::new()
            .layer(TimeoutLayer::new(Duration::from_secs(10)))
            .build_service(cloudwatch)
            .expect("This is a bug, no service spawning");

        let sink = {
            let svc_sink = ServiceSink::new(svc).batched(Vec::new(), self.buffer_size);
            Box::new(svc_sink)
        };

        let healthcheck = healthcheck(self.clone());

        Ok((sink, healthcheck))
    }
}

impl CloudwatchLogsSvc {
    pub fn new(config: CloudwatchLogsSinkConfig) -> Result<Self, ParseRegionError> {
        let client = CloudWatchLogsClient::new(config.region.clone());

        Ok(CloudwatchLogsSvc {
            client,
            config,
            state: State::Idle,
        })
    }

    fn put_logs(
        &mut self,
        sequence_token: Option<String>,
        records: Vec<Record>,
    ) -> RusotoFuture<PutLogEventsResponse, PutLogEventsError> {
        let log_events = records.into_iter().map(Into::into).collect();

        let request = PutLogEventsRequest {
            log_events,
            sequence_token,
            log_group_name: self.config.group_name.clone(),
            log_stream_name: self.config.stream_name.clone(),
        };

        self.client.put_log_events(request)
    }

    fn describe_stream(
        &mut self,
    ) -> RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError> {
        let request = DescribeLogStreamsRequest {
            limit: Some(1),
            log_group_name: self.config.group_name.clone(),
            log_stream_name_prefix: Some(self.config.stream_name.clone()),
            ..Default::default()
        };

        self.client.describe_log_streams(request)
    }
}

impl Service<Vec<Record>> for CloudwatchLogsSvc {
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

                    let stream = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                        .ok_or(CloudwatchError::NoStreamsFound)?;

                    self.state = State::Token(stream.upload_sequence_token);
                    return Ok(Async::Ready(()));
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

    fn call(&mut self, req: Vec<Record>) -> Self::Future {
        match &mut self.state {
            State::Token(token) => {
                let token = token.take();
                let (tx, rx) = oneshot::channel();
                self.state = State::Put(rx);

                let fut = self
                    .put_logs(token, req.into())
                    .map_err(CloudwatchError::Put)
                    .and_then(move |res| tx.send(res).map_err(|_| CloudwatchError::ServiceDropped));

                Box::new(fut)
            }
            _ => panic!("You did not call poll_ready!"),
        }
    }
}

fn healthcheck(config: CloudwatchLogsSinkConfig) -> super::Healthcheck {
    let region = config.region.clone();

    let client = CloudWatchLogsClient::new(region);

    let request = DescribeLogStreamsRequest {
        limit: Some(1),
        log_group_name: config.group_name.clone(),
        log_stream_name_prefix: Some(config.stream_name.clone()),
        ..Default::default()
    };

    let expected_stream = config.stream_name.clone();

    let fut = client
        .describe_log_streams(request)
        .map_err(|e| format!("DescribeLogStreams failed: {}", e))
        .and_then(|response| {
            response
                .log_streams
                .ok_or_else(|| "No streams found".to_owned())
        })
        .and_then(|streams| {
            streams
                .into_iter()
                .next()
                .ok_or_else(|| "No streams found".to_owned())
        })
        .and_then(|stream| {
            stream
                .log_stream_name
                .ok_or_else(|| "No stream name found but found a stream".to_owned())
        })
        .and_then(move |stream_name| {
            if stream_name == expected_stream {
                Ok(())
            } else {
                Err(format!(
                    "Stream returned is not the same as the one passed in got: {}, expected: {}",
                    stream_name, expected_stream
                ))
            }
        });

    Box::new(fut)
}

impl fmt::Display for CloudwatchError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CloudwatchError::Put(e) => write!(f, "CloudwatchError::Put: {}", e),
            CloudwatchError::Describe(e) => write!(f, "CloudwatchError::Describe: {}", e),
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
            CloudwatchError::ServiceDropped => write!(
                f,
                "CloudwatchError: The service was dropped while there was a request in flight."
            ),
        }
    }
}

impl std::error::Error for CloudwatchError {}

impl From<Record> for InputLogEvent {
    fn from(record: Record) -> InputLogEvent {
        InputLogEvent {
            message: record.line,
            timestamp: record.timestamp.timestamp_millis(),
        }
    }
}

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
    #![cfg(feature = "cloudwatch-integration-tests")]

    use crate::{
        sinks::cloudwatch::CloudwatchLogsSinkConfig,
        test_util::{block_on, random_lines_with_stream},
        topology::config::SinkConfig,
    };
    use futures::Sink;
    use rusoto_core::Region;
    use rusoto_logs::{
        CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupRequest, CreateLogStreamRequest,
        GetLogEventsRequest,
    };

    const STREAM_NAME: &'static str = "test-1";
    const GROUP_NAME: &'static str = "router";

    #[test]
    fn cloudwatch_insert_log_event() {
        let region = Region::Custom {
            name: "localstack".into(),
            endpoint: "http://localhost:6000".into(),
        };

        ensure_stream(region.clone());

        let config = CloudwatchLogsSinkConfig {
            stream_name: STREAM_NAME.into(),
            group_name: GROUP_NAME.into(),
            region: region.clone(),
            buffer_size: 1,
        };

        let (sink, _) = config.build().unwrap();

        let timestamp = chrono::Utc::now();

        let (input_lines, records) = random_lines_with_stream(100, 11);

        let pump = sink.send_all(records);
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

    fn ensure_stream(region: Region) {
        let client = CloudWatchLogsClient::new(region);

        let req = CreateLogGroupRequest {
            log_group_name: GROUP_NAME.into(),
            ..Default::default()
        };

        match client.create_log_group(req).sync() {
            Ok(_) => (),
            Err(_) => (),
        };

        let req = CreateLogStreamRequest {
            log_group_name: GROUP_NAME.into(),
            log_stream_name: STREAM_NAME.into(),
        };

        match client.create_log_stream(req).sync() {
            Ok(_) => (),
            Err(_) => (),
        };
    }

}
