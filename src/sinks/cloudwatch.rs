use crate::record::Record;
use futures::{future, try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use rusoto_core::{region::ParseRegionError, Region, RusotoFuture};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};

pub struct CloudwatchSink {
    buffer: Buffer<InputLogEvent>,
    stream_token: Option<String>,
    client: CloudWatchLogsClient,
    config: CloudwatchSinkConfig,
    state: State,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CloudwatchSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    pub region: String,
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

#[typetag::serde(name = "cloudwatch")]
impl crate::topology::config::SinkConfig for CloudwatchSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = CloudwatchSink::new(self.clone())
            .map_err(|e| format!("Failed to create CloudwatchSink: {}", e))?;
        let healthcheck = healthcheck(self.clone());

        Ok((Box::new(sink), healthcheck))
    }
}

impl CloudwatchSink {
    pub fn new(config: CloudwatchSinkConfig) -> Result<Self, ParseRegionError> {
        let buffer = Buffer::new(config.buffer_size);
        let region = config.region.clone().parse::<Region>()?;
        let client = CloudWatchLogsClient::new(region);

        Ok(CloudwatchSink {
            buffer,
            client,
            config,
            state: State::Buffering,
            stream_token: None,
        })
    }

    fn send_request(&mut self) {
        if let Some(token) = self.stream_token.take() {
            // If we already have a next_token then we can send the logs
            let log_events = self.buffer.flush();
            let request = PutLogEventsRequest {
                log_events,
                log_group_name: self.config.group_name.clone(),
                log_stream_name: self.config.stream_name.clone(),
                sequence_token: Some(token),
            };

            let fut = self.client.put_log_events(request);

            self.state = State::Put(fut);
        } else {
            // We have no next_token so lets fetch it first then send the logs
            let request = DescribeLogStreamsRequest {
                limit: Some(1),
                log_group_name: self.config.group_name.clone(),
                log_stream_name_prefix: Some(self.config.stream_name.clone()),
                ..Default::default()
            };

            let fut = self.client.describe_log_streams(request);

            self.state = State::Describe(fut);
        }
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

                    let log_events = self.buffer.flush();

                    let request = PutLogEventsRequest {
                        log_events,
                        log_group_name: self.config.group_name.clone(),
                        log_stream_name: self.config.stream_name.clone(),
                        sequence_token: token,
                    };

                    let fut = self.client.put_log_events(request);

                    self.state = State::Put(fut);
                    continue;
                }
                State::Buffering => unreachable!("This is a bug!"),
            }
        }
    }
}

impl Sink for CloudwatchSink {
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
                        self.send_request();
                        continue;
                    } else {
                        // check timer here???
                        // Buffer isnt full and there isn't an inflight request
                        if !self.buffer.empty() {
                            // Buffer isnt empty, isnt full and there is no inflight
                            // so lets take the rest of the buffer and send it.
                            self.send_request();
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

fn healthcheck(config: CloudwatchSinkConfig) -> super::Healthcheck {
    let region = config
        .region
        .clone()
        .parse::<Region>()
        .map_err(|e| format!("Region Not Valid: {}", e));

    let region = match region {
        Ok(region) => region,
        Err(e) => return Box::new(future::err(e)),
    };

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
