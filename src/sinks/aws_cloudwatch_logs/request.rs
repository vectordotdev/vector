use super::CloudwatchError;
use futures::compat::Compat;
use futures::future::BoxFuture;
use futures01::{sync::oneshot, try_ready, Async, Future, Poll};
use rusoto_core::RusotoError;
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
    events: Vec<Vec<InputLogEvent>>,
    token_tx: Option<oneshot::Sender<Option<String>>>,
}

struct Client {
    client: CloudWatchLogsClient,
    stream_name: String,
    group_name: String,
}

type ClientResult<T, E> = Compat<BoxFuture<'static, Result<T, RusotoError<E>>>>;

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
                            .pop()
                            .expect("Token got called multiple times, this is a bug!");

                        let token = stream.upload_sequence_token;

                        info!(message = "putting logs.", ?token);
                        self.state = State::Put(self.client.put_logs(token, events));
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

                    if let Some(events) = self.events.pop() {
                        debug!(message = "putting logs.", ?next_token);
                        self.state = State::Put(self.client.put_logs(next_token, events));
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
        log_events: Vec<InputLogEvent>,
    ) -> ClientResult<PutLogEventsResponse, PutLogEventsError> {
        let request = PutLogEventsRequest {
            log_events,
            sequence_token,
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        let client = self.client.clone();
        Compat::new(Box::pin(
            async move { client.put_log_events(request).await },
        ))
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
        Compat::new(Box::pin(async move {
            client.describe_log_streams(request).await
        }))
    }

    pub fn create_log_group(&self) -> ClientResult<(), CreateLogGroupError> {
        let request = CreateLogGroupRequest {
            log_group_name: self.group_name.clone(),
            ..Default::default()
        };

        let client = self.client.clone();
        Compat::new(Box::pin(
            async move { client.create_log_group(request).await },
        ))
    }

    pub fn create_log_stream(&self) -> ClientResult<(), CreateLogStreamError> {
        let request = CreateLogStreamRequest {
            log_group_name: self.group_name.clone(),
            log_stream_name: self.stream_name.clone(),
        };

        let client = self.client.clone();
        Compat::new(Box::pin(
            async move { client.create_log_stream(request).await },
        ))
    }
}
