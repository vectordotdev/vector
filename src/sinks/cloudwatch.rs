use crate::record::Record;
use futures::future::{self, Either};
use futures::{try_ready, Async, AsyncSink, Future, Poll, Sink, StartSend};
use rusoto_core::{region::ParseRegionError, Region, RusotoFuture};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, DescribeLogStreamsError, DescribeLogStreamsRequest,
    DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError, PutLogEventsRequest,
    PutLogEventsResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub struct CloudwatchSink {
    buffer: Buffer<InputLogEvent>,
    stream_token: Option<String>,
    in_flight: Option<RequestFuture>,
    client: Arc<CloudWatchLogsClient>,
    config: CloudwatchSinkConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CloudwatchSinkConfig {
    pub stream_name: String,
    pub group_name: String,
    pub region: Option<String>,
    pub buffer_size: usize,
}

struct RequestFuture {
    client: Arc<CloudWatchLogsClient>,
    log_events: Option<Vec<InputLogEvent>>,
    state: State,
    stream_name: String,
    group_name: String,
}

enum State {
    Describe(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(RusotoFuture<PutLogEventsResponse, PutLogEventsError>),
}

#[derive(Debug)]
enum Error {
    Put(PutLogEventsError),
    Describe(DescribeLogStreamsError),
    NoStreamsFound,
    NoToken,
}

#[typetag::serde(name = "cloudwatch")]
impl crate::topology::config::SinkConfig for CloudwatchSinkConfig {
    fn build(&self) -> Result<(super::RouterSink, super::Healthcheck), String> {
        let sink = CloudwatchSink::new(self.clone())
            .map_err(|e| format!("Failed to create CloudwatchSink: {}", e))?;
        let healthcheck = healthcheck(self.clone());

        Ok((Box::new(sink), Box::new(healthcheck)))
    }
}

fn healthcheck(config: CloudwatchSinkConfig) -> impl Future<Item = (), Error = String> {
    let region = config
        .region
        .clone()
        .expect("Must set a region for Cloudwatch")
        .parse::<Region>()
        .map_err(|e| format!("Region Not Valid: {}", e));

    let region = match region {
        Ok(region) => region,
        Err(e) => return Either::A(future::err(e)),
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

    Either::B(fut)
}

impl CloudwatchSink {
    pub fn new(config: CloudwatchSinkConfig) -> Result<Self, ParseRegionError> {
        let buffer = Buffer::new(config.buffer_size);
        let region = config
            .region
            .clone()
            .expect("Must set a region for Cloudwatch")
            .parse::<Region>()?;
        let client = Arc::new(CloudWatchLogsClient::new(region));

        Ok(CloudwatchSink {
            buffer,
            client,
            config,
            stream_token: None,
            in_flight: None,
        })
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
            if let Some(ref mut fut) = self.in_flight {
                match fut.poll() {
                    Ok(Async::Ready(token)) => {
                        self.in_flight = None;
                        self.stream_token = token;
                    }
                    Ok(Async::NotReady) => return Ok(Async::NotReady),
                    Err(e) => panic!("Error sending logs to cloudwatch: {:?}", e),
                }
            } else {
                if self.buffer.full() {
                    let fut = RequestFuture::new(
                        self.client.clone(),
                        self.config.group_name.clone(),
                        self.config.stream_name.clone(),
                        self.stream_token.clone(),
                        self.buffer.flush(),
                    );

                    self.in_flight = Some(fut);
                    return Ok(Async::NotReady);
                } else {
                    // check timer here???
                    // Buffer isnt full and there isn't an inflight request
                    if !self.buffer.empty() {
                        // Buffer isnt empty, isnt full and there is no inflight
                    } else {
                        return Ok(Async::Ready(()));
                    }
                }
            }
        }
    }
}

impl RequestFuture {
    pub fn new(
        client: Arc<CloudWatchLogsClient>,
        group_name: String,
        stream_name: String,
        next_token: Option<String>,
        log_events: Vec<InputLogEvent>,
    ) -> Self {
        if let Some(token) = next_token {
            // If we already have a next_token then we can send the logs
            let request = PutLogEventsRequest {
                log_events,
                log_group_name: group_name.clone(),
                log_stream_name: stream_name.clone(),
                sequence_token: Some(token),
            };

            let fut = client.put_log_events(request);

            RequestFuture {
                client,
                log_events: None,
                group_name,
                stream_name,
                state: State::Put(fut),
            }
        } else {
            // We have no next_token so lets fetch it first then send the logs
            let request = DescribeLogStreamsRequest {
                limit: Some(1),
                log_group_name: group_name.clone(),
                log_stream_name_prefix: Some(stream_name.clone()),
                ..Default::default()
            };

            let fut = client.describe_log_streams(request);

            RequestFuture {
                client,
                log_events: Some(log_events),
                group_name,
                stream_name,
                state: State::Describe(fut),
            }
        }
    }
}

impl Future for RequestFuture {
    type Item = Option<String>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match self.state {
                State::Put(ref mut fut) => {
                    let response = try_ready!(fut.poll());

                    return Ok(Async::Ready(response.next_sequence_token));
                }
                State::Describe(ref mut fut) => {
                    let response = try_ready!(fut.poll());

                    // TODO(lucio): verify if this is the right approach
                    let _stream = response
                        .log_streams
                        .ok_or(Error::NoStreamsFound)?
                        .into_iter()
                        .next()
                        .ok_or(Error::NoStreamsFound)?;

                    let token = response.next_token.ok_or(Error::NoToken)?;

                    let log_events = self
                        .log_events
                        .take()
                        .expect("Describe events was sent twice! this is a bug");

                    let request = PutLogEventsRequest {
                        log_events,
                        log_group_name: self.group_name.clone(),
                        log_stream_name: self.stream_name.clone(),
                        sequence_token: Some(token),
                    };

                    let fut = self.client.put_log_events(request);

                    self.state = State::Put(fut);
                    continue;
                }
            }
        }
    }
}

impl From<Record> for InputLogEvent {
    fn from(record: Record) -> InputLogEvent {
        InputLogEvent {
            message: record.line,
            timestamp: record.timestamp.timestamp(),
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

impl<T> Buffer<T> {
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
