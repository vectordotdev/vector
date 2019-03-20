use crate::{
    record::Record,
    sinks::util::{RecordBuffer, ServiceSink, SinkExt},
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
use std::sync::Arc;
use std::time::Duration;
use tower_service::Service;
use tower_timeout::Timeout;

pub struct CloudwatchLogsSvc {
    stream_token: Option<String>,
    client: Arc<CloudWatchLogsClient>,
    in_flight: Option<oneshot::Receiver<PutLogEventsResponse>>,
    config: CloudwatchLogsSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CloudwatchLogsSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    pub region: Region,
    pub buffer_size: usize,
}

pub struct CloudwatchFuture {
    client: Arc<CloudWatchLogsClient>,
    config: CloudwatchLogsSinkConfig,
    records: Option<Vec<Record>>,
    tx: Option<oneshot::Sender<PutLogEventsResponse>>,
    state: State,
}

enum State {
    Describe(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(RusotoFuture<PutLogEventsResponse, PutLogEventsError>),
}

#[derive(Debug)]
pub enum CloudwatchError {
    Put(PutLogEventsError),
    Describe(DescribeLogStreamsError),
    NoStreamsFound,
}

#[typetag::serde(name = "cloudwatch_logs")]
impl crate::topology::config::SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let svc = CloudwatchLogsSvc::new(self.clone()).map_err(|e| e.description().to_string())?;
        let svc = Timeout::new(svc, Duration::from_secs(10));
        let sink = {
            let svc_sink = ServiceSink::new(svc).batched(RecordBuffer::default(), self.buffer_size);
            Box::new(svc_sink)
        };

        let healthcheck = healthcheck(self.clone());

        Ok((sink, healthcheck))
    }
}

impl CloudwatchLogsSvc {
    pub fn new(config: CloudwatchLogsSinkConfig) -> Result<Self, ParseRegionError> {
        let client = Arc::new(CloudWatchLogsClient::new(config.region.clone()));

        Ok(CloudwatchLogsSvc {
            client,
            config,
            in_flight: None,
            stream_token: None,
        })
    }

    fn put_logs(
        client: Arc<CloudWatchLogsClient>,
        config: &CloudwatchLogsSinkConfig,
        records: Vec<Record>,
        token: Option<String>,
    ) -> RusotoFuture<PutLogEventsResponse, PutLogEventsError> {
        let log_events = records.into_iter().map(Into::into).collect();

        let request = PutLogEventsRequest {
            log_events,
            log_group_name: config.group_name.clone(),
            log_stream_name: config.stream_name.clone(),
            sequence_token: token,
        };

        client.put_log_events(request)
    }

    fn describe_stream(
        client: Arc<CloudWatchLogsClient>,
        config: &CloudwatchLogsSinkConfig,
    ) -> RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError> {
        let request = DescribeLogStreamsRequest {
            limit: Some(1),
            log_group_name: config.group_name.clone(),
            log_stream_name_prefix: Some(config.stream_name.clone()),
            ..Default::default()
        };

        client.describe_log_streams(request)
    }

    fn send_request(
        &mut self,
        records: Vec<Record>,
        tx: oneshot::Sender<PutLogEventsResponse>,
    ) -> CloudwatchFuture {
        // FIXME: the token here will always be None until we can force the service
        // to send one request at a time and use the return value of the previous.
        CloudwatchFuture::new(
            self.client.clone(),
            self.stream_token.take(),
            self.config.clone(),
            tx,
            records,
        )
    }
}

impl Service<RecordBuffer> for CloudwatchLogsSvc {
    type Response = ();
    type Error = CloudwatchError;
    type Future = CloudwatchFuture;

    fn poll_ready(&mut self) -> Poll<(), Self::Error> {
        if let Some(in_flight) = &mut self.in_flight {
            let response = match in_flight.poll() {
                Ok(Async::Ready(response)) => response,
                Ok(Async::NotReady) => return Ok(Async::NotReady),
                Err(_) => panic!("The in flight future was dropped!"),
            };
            self.stream_token = response.next_sequence_token;
        }

        Ok(().into())
    }

    fn call(&mut self, req: RecordBuffer) -> Self::Future {
        let (tx, rx) = oneshot::channel();
        self.in_flight = Some(rx);
        self.send_request(req.into(), tx)
    }
}

impl CloudwatchFuture {
    pub fn new(
        client: Arc<CloudWatchLogsClient>,
        stream_token: Option<String>,
        config: CloudwatchLogsSinkConfig,
        tx: oneshot::Sender<PutLogEventsResponse>,
        records: Vec<Record>,
    ) -> Self {
        if let Some(token) = stream_token {
            let fut = CloudwatchLogsSvc::put_logs(client.clone(), &config, records, Some(token));
            let state = State::Put(fut);
            CloudwatchFuture {
                client,
                records: None,
                config,
                tx: Some(tx),
                state,
            }
        } else {
            let fut = CloudwatchLogsSvc::describe_stream(client.clone(), &config);
            let state = State::Describe(fut);
            CloudwatchFuture {
                client,
                records: Some(records),
                config,
                tx: Some(tx),
                state,
            }
        }
    }
}

impl Future for CloudwatchFuture {
    type Item = ();
    type Error = CloudwatchError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                State::Put(ref mut fut) => {
                    // TODO(lucio): invesitgate failure cases on rejected logs
                    let response = try_ready!(fut.poll());

                    self.tx.take().unwrap().send(response).unwrap();

                    return Ok(Async::Ready(()));
                }
                State::Describe(ref mut fut) => {
                    let response = try_ready!(fut.poll());

                    let stream = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                        .ok_or(CloudwatchError::NoStreamsFound)?;

                    let token = stream.upload_sequence_token;

                    let records = self.records.take().unwrap();
                    let fut = CloudwatchLogsSvc::put_logs(
                        self.client.clone(),
                        &self.config,
                        records,
                        token,
                    );
                    self.state = State::Put(fut);
                    continue;
                }
            }
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
            CloudwatchError::Put(e) => write!(f, "CloudwatchError: {}", e),
            CloudwatchError::Describe(e) => write!(f, "CloudwatchError: {}", e),
            CloudwatchError::NoStreamsFound => write!(f, "CloudwatchError: No Streams Found"),
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

    use crate::sinks::cloudwatch::CloudwatchLogsSinkConfig;
    use crate::test_util::{block_on, random_lines};
    use crate::topology::config::SinkConfig;
    use crate::Record;
    use futures::{future::poll_fn, stream, Sink};
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

        let input_lines = random_lines(100).take(11).collect::<Vec<_>>();
        let records = input_lines
            .iter()
            .map(|line| Record::from(line.clone()))
            .collect::<Vec<_>>();

        let pump = sink.send_all(stream::iter_ok(records.into_iter()));

        let (mut sink, _) = block_on(pump).unwrap();

        block_on(poll_fn(move || sink.close())).unwrap();

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
