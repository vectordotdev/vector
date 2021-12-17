use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, ready, FutureExt};
use rusoto_core::{RusotoError, RusotoResult};
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogGroupError, CreateLogGroupRequest,
    CreateLogStreamError, CreateLogStreamRequest, DescribeLogStreamsError,
    DescribeLogStreamsRequest, DescribeLogStreamsResponse, InputLogEvent, PutLogEventsError,
    PutLogEventsRequest, PutLogEventsResponse,
};
use tokio::sync::oneshot;

use crate::sinks::aws_cloudwatch_logs::service::CloudwatchError;

pub struct CloudwatchFuture {
    client: Client,
    state: State,
    create_missing_group: bool,
    create_missing_stream: bool,
    events: Vec<Vec<InputLogEvent>>,
    token_tx: Option<oneshot::Sender<Option<String>>>,
}

struct Client {
    client: CloudWatchLogsClient,
    stream_name: String,
    group_name: String,
}

type ClientResult<T, E> = BoxFuture<'static, RusotoResult<T, E>>;

enum State {
    CreateGroup(ClientResult<(), CreateLogGroupError>),
    CreateStream(ClientResult<(), CreateLogStreamError>),
    DescribeStream(ClientResult<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(ClientResult<PutLogEventsResponse, PutLogEventsError>),
}

impl CloudwatchFuture {
    /// Panics if events.is_empty()
    pub fn new(
        client: CloudWatchLogsClient,
        stream_name: String,
        group_name: String,
        create_missing_group: bool,
        create_missing_stream: bool,
        mut events: Vec<Vec<InputLogEvent>>,
        token: Option<String>,
        token_tx: oneshot::Sender<Option<String>>,
    ) -> Self {
        let client = Client {
            client,
            stream_name,
            group_name,
        };

        let state = if let Some(token) = token {
            State::Put(client.put_logs(Some(token), events.pop().expect("No Events to send")))
        } else {
            State::DescribeStream(client.describe_stream())
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
    type Output = Result<(), CloudwatchError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Self::Output> {
        loop {
            match &mut self.state {
                State::DescribeStream(fut) => {
                    let response = match ready!(fut.poll_unpin(cx)) {
                        Ok(response) => response,
                        Err(RusotoError::Service(DescribeLogStreamsError::ResourceNotFound(_)))
                            if self.create_missing_group =>
                        {
                            info!("Log group provided does not exist; creating a new one.");

                            self.state = State::CreateGroup(self.client.create_log_group());
                            continue;
                        }
                        Err(err) => return Poll::Ready(Err(CloudwatchError::Describe(err))),
                    };

                    if let Some(stream) = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                    {
                        debug!(message = "Stream found.", stream = ?stream.log_stream_name);

                        let events = self
                            .events
                            .pop()
                            .expect("Token got called multiple times, self is a bug!");

                        let token = stream.upload_sequence_token;

                        info!(message = "Putting logs.", token = ?token);
                        self.state = State::Put(self.client.put_logs(token, events));
                    } else if self.create_missing_stream {
                        info!("Provided stream does not exist; creating a new one.");
                        self.state = State::CreateStream(self.client.create_log_stream());
                    } else {
                        return Poll::Ready(Err(CloudwatchError::NoStreamsFound));
                    }
                }

                State::CreateGroup(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        Ok(_) => {}
                        Err(RusotoError::Service(CreateLogGroupError::ResourceAlreadyExists(
                            _,
                        ))) => {}
                        Err(err) => return Poll::Ready(Err(CloudwatchError::CreateGroup(err))),
                    };

                    info!(message = "Group created.", name = %self.client.group_name);

                    // self does not abide by `create_missing_stream` since a group
                    // never has any streams and thus we need to create one if a group
                    // is created no matter what.
                    self.state = State::CreateStream(self.client.create_log_stream());
                }

                State::CreateStream(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        Ok(_) => {}
                        Err(RusotoError::Service(CreateLogStreamError::ResourceAlreadyExists(
                            _,
                        ))) => {}
                        Err(err) => return Poll::Ready(Err(CloudwatchError::CreateStream(err))),
                    };

                    info!(message = "Stream created.", name = %self.client.stream_name);

                    self.state = State::DescribeStream(self.client.describe_stream());
                }

                State::Put(fut) => {
                    let next_token = match ready!(fut.poll_unpin(cx)) {
                        Ok(resp) => resp.next_sequence_token,
                        Err(err) => return Poll::Ready(Err(CloudwatchError::Put(err))),
                    };

                    if let Some(events) = self.events.pop() {
                        debug!(message = "Putting logs.", next_token = ?next_token);
                        self.state = State::Put(self.client.put_logs(next_token, events));
                    } else {
                        info!(message = "Putting logs was successful.", next_token = ?next_token);

                        self.token_tx
                            .take()
                            .expect("Put was polled after finishing.")
                            .send(next_token)
                            .expect("CloudwatchLogsSvc was dropped unexpectedly");

                        return Poll::Ready(Ok(()));
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
        log_events: Vec<InputLogEvent>,
    ) -> ClientResult<PutLogEventsResponse, PutLogEventsError> {
        let request = PutLogEventsRequest {
            log_events,
            sequence_token,
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        let client = self.client.clone();
        Box::pin(async move { client.put_log_events(request).await })
    }

    pub fn describe_stream(
        &self,
    ) -> ClientResult<DescribeLogStreamsResponse, DescribeLogStreamsError> {
        let request = DescribeLogStreamsRequest {
            limit: Some(1),
            log_group_name: self.group_name.clone(),
            log_stream_name_prefix: Some(self.stream_name.clone()),
            ..Default::default()
        };

        let client = self.client.clone();
        Box::pin(async move { client.describe_log_streams(request).await })
    }

    pub fn create_log_group(&self) -> ClientResult<(), CreateLogGroupError> {
        let request = CreateLogGroupRequest {
            log_group_name: self.group_name.clone(),
            ..Default::default()
        };

        let client = self.client.clone();
        Box::pin(async move { client.create_log_group(request).await })
    }

    pub fn create_log_stream(&self) -> ClientResult<(), CreateLogStreamError> {
        let request = CreateLogStreamRequest {
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        let client = self.client.clone();
        Box::pin(async move { client.create_log_stream(request).await })
    }
}
