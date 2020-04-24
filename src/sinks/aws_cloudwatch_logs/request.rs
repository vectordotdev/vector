use super::CloudwatchError;
use chrono::Duration;
use futures01::{sync::oneshot, try_ready, Async, Future, Poll};
use rusoto_core::{RusotoError, RusotoFuture};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogGroupRequest,
    CreateLogStreamError, CreateLogStreamRequest, DescribeLogStreamsError,
    DescribeLogStreamsRequest, DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError,
    PutLogEventsRequest, PutLogEventsResponse,
};

pub struct CloudwatchFuture {
    client: Client,
    state: State,
    create_missing_group: bool,
    create_missing_stream: bool,
    events: Option<Vec<InputLogEvent>>,
    token_tx: Option<oneshot::Sender<Option<String>>>,
}

struct Client {
    client: CloudWatchLogsClient,
    stream_name: String,
    group_name: String,
}

enum State {
    CreateGroup(RusotoFuture<(), CreateLogGroupError>),
    CreateStream(RusotoFuture<(), CreateLogStreamError>),
    DescribeStream(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(RusotoFuture<PutLogEventsResponse, PutLogEventsError>),
}

impl CloudwatchFuture {
    pub fn new(
        client: CloudWatchLogsClient,
        stream_name: String,
        group_name: String,
        create_missing_group: bool,
        create_missing_stream: bool,
        events: Vec<InputLogEvent>,
        token: Option<String>,
        token_tx: oneshot::Sender<Option<String>>,
    ) -> Self {
        let client = Client {
            client,
            stream_name,
            group_name,
        };

        let (state, events) = if let Some(token) = token {
            let (request, events) = client.put_logs(Some(token), events);
            let state = State::Put(request);
            (state, events)
        } else {
            let state = State::DescribeStream(client.describe_stream());
            (state, Some(events))
        };

        Self {
            client,
            events,
            state,
            token_tx: Some(token_tx),
            create_missing_group,
            create_missing_stream,
        }
    }
}

impl Future for CloudwatchFuture {
    type Item = ();
    type Error = CloudwatchError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            match &mut self.state {
                State::DescribeStream(fut) => {
                    let response = match fut.poll() {
                        Ok(Async::Ready(res)) => res,
                        Ok(Async::NotReady) => return Ok(Async::NotReady),
                        Err(e) => {
                            if let RusotoError::Service(
                                DescribeLogStreamsError::ResourceNotFound(_),
                            ) = e
                            {
                                if self.create_missing_group {
                                    info!("log group provided does not exist; creating a new one.");

                                    self.state = State::CreateGroup(self.client.create_log_group());
                                    continue;
                                } else {
                                    return Err(CloudwatchError::Describe(e));
                                }
                            } else {
                                return Err(CloudwatchError::Describe(e));
                            }
                        }
                    };

                    if let Some(stream) = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                    {
                        debug!(message = "stream found", stream = ?stream.log_stream_name);

                        let events = self
                            .events
                            .take()
                            .expect("Token got called twice, this is a bug!");

                        let token = stream.upload_sequence_token;

                        info!(message = "putting logs.", ?token);
                        let (request, events) = self.client.put_logs(token, events);
                        self.state = State::Put(request);
                        self.events = events;
                    } else if self.create_missing_stream {
                        info!("provided stream does not exist; creating a new one.");
                        self.state = State::CreateStream(self.client.create_log_stream());
                    } else {
                        return Err(CloudwatchError::NoStreamsFound);
                    }
                }

                State::CreateGroup(fut) => {
                    try_ready!(fut
                        .poll()
                        .or_else(|e| {
                            if let RusotoError::Service(
                                CreateLogGroupError::ResourceAlreadyExists(_),
                            ) = e
                            {
                                Ok(Async::Ready(()))
                            } else {
                                Err(e)
                            }
                        })
                        .map_err(CloudwatchError::CreateGroup));

                    info!(message = "group created.", name = %self.client.group_name);

                    // This does not abide by `create_missing_stream` since a group
                    // never has any streams and thus we need to create one if a group
                    // is created no matter what.
                    self.state = State::CreateStream(self.client.create_log_stream());
                }

                State::CreateStream(fut) => {
                    try_ready!(fut
                        .poll()
                        .or_else(|e| {
                            if let RusotoError::Service(
                                CreateLogStreamError::ResourceAlreadyExists(_),
                            ) = e
                            {
                                Ok(Async::Ready(()))
                            } else {
                                Err(e)
                            }
                        })
                        .map_err(CloudwatchError::CreateStream));

                    info!(message = "stream created.", name = %self.client.stream_name);

                    self.state = State::DescribeStream(self.client.describe_stream());
                }

                State::Put(fut) => {
                    let res = try_ready!(fut.poll().map_err(CloudwatchError::Put));

                    let next_token = res.next_sequence_token;

                    if let Some(events) = self.events.take() {
                        info!(message = "putting logs.", ?next_token);
                        let (request, events) = self.client.put_logs(next_token, events);
                        self.state = State::Put(request);
                        self.events = events;
                    } else {
                        info!(message = "putting logs was successful.", ?next_token);

                        self.token_tx
                            .take()
                            .expect("Put was polled after finishing.")
                            .send(next_token)
                            .expect("CloudwatchLogsSvc was dropped unexpectedly");

                        return Ok(().into());
                    }
                }
            }
        }
    }
}

impl Client {
    pub fn put_logs(
        &self,
        sequence_token: Option<String>,
        mut log_events: Vec<InputLogEvent>,
    ) -> (
        RusotoFuture<PutLogEventsResponse, PutLogEventsError>,
        Option<Vec<InputLogEvent>>,
    ) {
        // We will only send events that happened in the 24h window since the oldest event.
        let remainder = log_events
            .first()
            .map(|oldest| oldest.timestamp + Duration::days(1).num_milliseconds())
            .and_then(|limit| {
                log_events
                    .iter()
                    .enumerate()
                    .find(|e| e.1.timestamp >= limit)
                    .map(|(at, _)| at)
                    .map(|at| log_events.drain(at..).collect::<Vec<_>>())
            });

        let request = PutLogEventsRequest {
            log_events,
            sequence_token,
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        (self.client.put_log_events(request), remainder)
    }

    pub fn describe_stream(
        &self,
    ) -> RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError> {
        let request = DescribeLogStreamsRequest {
            limit: Some(1),
            log_group_name: self.group_name.clone(),
            log_stream_name_prefix: Some(self.stream_name.clone()),
            ..Default::default()
        };

        self.client.describe_log_streams(request)
    }

    pub fn create_log_group(&self) -> RusotoFuture<(), CreateLogGroupError> {
        let request = CreateLogGroupRequest {
            log_group_name: self.group_name.clone(),
            ..Default::default()
        };

        self.client.create_log_group(request)
    }

    pub fn create_log_stream(&self) -> RusotoFuture<(), CreateLogStreamError> {
        let request = CreateLogStreamRequest {
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        self.client.create_log_stream(request)
    }
}
