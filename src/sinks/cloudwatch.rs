use crate::record::Record;
use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use rusoto_core::{region::ParseRegionError, Region, RusotoFuture};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};

pub struct CloudwatchLogsSink {
    buffer: Buffer<InputLogEvent>,
    stream_token: Option<String>,
    client: CloudWatchLogsClient,
    config: CloudwatchLogsSinkConfig,
    state: State,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CloudwatchLogsSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    pub region: Region,
    pub buffer_size: usize,
}

enum State {
    Describe(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(RusotoFuture<PutLogEventsResponse, PutLogEventsError>),
    Buffering,
}

#[derive(Debug)]
enum Error {
    Put(PutLogEventsError),
    Describe(DescribeLogStreamsError),
    NoStreamsFound,
}

#[typetag::serde(name = "cloudwatch_logs")]
impl crate::topology::config::SinkConfig for CloudwatchLogsSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = CloudwatchLogsSink::new(self.clone())
            .map_err(|e| format!("Failed to create CloudwatchLogsSink: {}", e))?;
        let healthcheck = healthcheck(self.clone());

        Ok((Box::new(sink), healthcheck))
    }
}

impl CloudwatchLogsSink {
    pub fn new(config: CloudwatchLogsSinkConfig) -> Result<Self, ParseRegionError> {
        let buffer = Buffer::new(config.buffer_size);
        let client = CloudWatchLogsClient::new(config.region.clone());

        Ok(CloudwatchLogsSink {
            buffer,
            client,
            config,
            state: State::Buffering,
            stream_token: None,
        })
    }

    fn put_logs(
        &mut self,
        token: Option<String>,
    ) -> RusotoFuture<PutLogEventsResponse, PutLogEventsError> {
        let log_events = self.buffer.flush();
        let request = PutLogEventsRequest {
            log_events,
            log_group_name: self.config.group_name.clone(),
            log_stream_name: self.config.stream_name.clone(),
            sequence_token: token,
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

    fn poll_request(&mut self) -> Poll<(), Error> {
        loop {
            match self.state {
                State::Put(ref mut fut) => {
                    // TODO(lucio): invesitgate failure cases on rejected logs
                    let response = try_ready!(fut.poll());

                    self.stream_token = response.next_sequence_token;
                    return Ok(().into());
                }
                State::Describe(ref mut fut) => {
                    let response = try_ready!(fut.poll());

                    let stream = response
                        .log_streams
                        .ok_or(Error::NoStreamsFound)?
                        .into_iter()
                        .next()
                        .ok_or(Error::NoStreamsFound)?;

                    let token = stream.upload_sequence_token;

                    let fut = self.put_logs(token);
                    self.state = State::Put(fut);
                    continue;
                }
                State::Buffering => {
                    if let Some(token) = self.stream_token.take() {
                        let fut = self.put_logs(Some(token));
                        self.state = State::Put(fut);
                        continue;
                    } else {
                        let fut = self.describe_stream();
                        self.state = State::Describe(fut);
                        continue;
                    }
                }
            }
        }
    }
}

impl Sink for CloudwatchLogsSink {
    type SinkItem = Record;
    type SinkError = ();

    fn start_send(&mut self, item: Self::SinkItem) -> StartSend<Self::SinkItem, Self::SinkError> {
        if self.buffer.full() {
            self.poll_complete()?;

            if self.buffer.full() {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        self.buffer.push(item.into());

        if self.buffer.full() {
            self.poll_complete()?;
        }

        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        loop {
            match self.state {
                State::Buffering => {
                    if self.buffer.full() {
                        try_ready!(self
                            .poll_request()
                            .map_err(|e| panic!("Error sending logs to cloudwatch: {:?}", e)));
                        continue;
                    } else {
                        // check timer here???
                        // Buffer isnt full and there isn't an inflight request
                        if !self.buffer.empty() {
                            // Buffer isnt empty, isnt full and there is no inflight
                            // so lets take the rest of the buffer and send it.
                            try_ready!(self
                                .poll_request()
                                .map_err(|e| panic!("Error sending logs to cloudwatch: {:?}", e)));
                            continue;
                        } else {
                            return Ok(Async::Ready(()));
                        }
                    }
                }

                State::Describe(_) | State::Put(_) => {
                    try_ready!(self
                        .poll_request()
                        .map_err(|e| panic!("Error sending logs to cloudwatch: {:?}", e)));

                    self.state = State::Buffering;
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

impl From<Record> for InputLogEvent {
    fn from(record: Record) -> InputLogEvent {
        InputLogEvent {
            message: record.line,
            timestamp: record.timestamp.timestamp_millis(),
        }
    }
}

impl From<PutLogEventsError> for Error {
    fn from(e: PutLogEventsError) -> Self {
        Error::Put(e)
    }
}

impl From<DescribeLogStreamsError> for Error {
    fn from(e: DescribeLogStreamsError) -> Self {
        Error::Describe(e)
    }
}

pub struct Buffer<T> {
    inner: Vec<T>,
    size: usize,
}

impl<T> Buffer<T>
where
    T: std::fmt::Debug,
{
    pub fn new(size: usize) -> Self {
        Buffer {
            inner: Vec::new(),
            size,
        }
    }

    pub fn full(&self) -> bool {
        self.inner.len() >= self.size
    }

    pub fn push(&mut self, item: T) {
        self.inner.push(item);
    }

    pub fn flush(&mut self) -> Vec<T> {
        // TODO(lucio): make this unsafe replace?
        self.inner.drain(..).collect()
    }

    pub fn empty(&self) -> bool {
        self.inner.is_empty()
    }
}

#[cfg(test)]
mod tests {
    #![cfg(feature = "cloudwatch-integration-tests")]

    use crate::sinks::cloudwatch::{CloudwatchLogsSink, CloudwatchLogsSinkConfig};
    use crate::test_util::{block_on, random_lines};
    use crate::Record;
    use futures::{future::poll_fn, stream, Sink};
    use rusoto_core::Region;
    use rusoto_logs::{CloudWatchLogs, CloudWatchLogsClient, GetLogEventsRequest};

    const STREAM_NAME: &'static str = "test-1";
    const GROUP_NAME: &'static str = "router";

    #[test]
    fn cloudwatch_insert_log_event() {
        let region = Region::UsEast1;

        let config = CloudwatchLogsSinkConfig {
            stream_name: STREAM_NAME.into(),
            group_name: GROUP_NAME.into(),
            region,
            buffer_size: 2,
        };

        let sink = CloudwatchLogsSink::new(config).unwrap();

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

        let client = CloudWatchLogsClient::new(Region::UsEast1);

        let response = block_on(client.get_log_events(request)).unwrap();

        let events = response.events.unwrap();

        let output_lines = events
            .into_iter()
            .map(|e| e.message.unwrap())
            .collect::<Vec<_>>();

        assert_eq!(output_lines, input_lines);
    }

}
