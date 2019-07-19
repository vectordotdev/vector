use super::CloudwatchError;
use futures::{sync::mpsc, Future, Poll};
use rusoto_core::RusotoFuture;
use rusoto_logs::{
    CloudWatchLogs, CloudWatchLogsClient, CreateLogStreamError, CreateLogStreamRequest,
    DescribeLogStreamsError, DescribeLogStreamsRequest, DescribeLogStreamsResponse, InputLogEvent,
    PutLogEventsError, PutLogEventsRequest, PutLogEventsResponse,
};

pub struct CloudwatchFuture {
    client: CloudWatchLogsClient,
    state: State,
    stream_name: String,
    group_name: String,
    events: Option<Vec<InputLogEvent>>,
    token: Option<String>,
    token_tx: mpsc::Sender<Option<String>>,
}

enum State {
    Idle,
    Token(Option<String>),
    CreateStream(RusotoFuture<(), CreateLogStreamError>),
    DescribeStream(RusotoFuture<DescribeLogStreamsResponse, DescribeLogStreamsError>),
    Put(RusotoFuture<PutLogEventsResponse, PutLogEventsError>),
}

impl CloudwatchFuture {
    pub fn new(
        client: CloudWatchLogsClient,
        stream_name: String,
        group_name: String,
        events: Vec<InputLogEvent>,
        token: Option<String>,
        token_tx: mpsc::Sender<Option<String>>,
    ) -> Self {
        Self {
            client,
            events: Some(events),
            state: State::Idle,
            stream_name,
            group_name,
            token,
            token_tx,
        }
    }

    fn put_logs(
        &mut self,
        sequence_token: Option<String>,
        log_events: Vec<InputLogEvent>,
    ) -> RusotoFuture<PutLogEventsResponse, PutLogEventsError> {
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
}

impl Future for CloudwatchFuture {
    type Item = ();
    type Error = CloudwatchError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        macro_rules! try_ready {
            ($e:expr) => {
                match $e {
                    Ok(futures::Async::Ready(t)) => t,
                    Ok(futures::Async::NotReady) => return Ok(futures::Async::NotReady),
                    Err(e) => {
                        self.token_tx.try_send(self.token.take()).unwrap();

                        return Err(From::from(e));
                    }
                }
            };
        }

        loop {
            match &mut self.state {
                State::Idle => {
                    if let Some(token) = self.token.take() {
                        self.state = State::Token(Some(token));
                        continue;
                    } else {
                        trace!("Token does not exist; calling describe stream.");
                        let fut = self.describe_stream();
                        self.state = State::DescribeStream(fut);
                        continue;
                    }
                }

                State::DescribeStream(fut) => {
                    let response = try_ready!(fut.poll().map_err(CloudwatchError::Describe));

                    let stream = if let Some(stream) = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .next()
                    {
                        trace!(message = "stream found", stream = ?stream.log_stream_name);
                        stream
                    } else {
                        trace!("provided stream does not exist; creating a new one.");
                        let fut = self.create_log_stream();
                        self.state = State::CreateStream(fut);
                        continue;
                    };

                    self.state = State::Token(stream.upload_sequence_token);
                }

                State::Token(token) => {
                    // These both take since this call should only happen once per
                    // future.
                    let token = token.take();
                    let events = self
                        .events
                        .take()
                        .expect("Token got called twice, this is a bug!");

                    trace!(message = "putting logs.", ?token);
                    let fut = self.put_logs(token, events);
                    self.state = State::Put(fut);
                    continue;
                }

                State::CreateStream(fut) => {
                    let _ = try_ready!(fut.poll().map_err(CloudwatchError::CreateStream));

                    trace!("stream created.");

                    self.state = State::Idle;
                    continue;
                }

                State::Put(fut) => {
                    let res = try_ready!(fut.poll().map_err(CloudwatchError::Put));

                    let next_token = res.next_sequence_token;

                    trace!(message = "putting logs was successful.", ?next_token);

                    self.token_tx.try_send(next_token).unwrap();

                    return Ok(().into());
                }
            }
        }
    }
}
