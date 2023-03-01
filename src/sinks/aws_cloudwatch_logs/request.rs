use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};

use aws_sdk_cloudwatchlogs::error::{
    CreateLogGroupError, CreateLogGroupErrorKind, CreateLogStreamError, CreateLogStreamErrorKind,
    DescribeLogStreamsError, DescribeLogStreamsErrorKind, PutLogEventsError,
};
use aws_sdk_cloudwatchlogs::operation::PutLogEvents;

use aws_sdk_cloudwatchlogs::model::InputLogEvent;
use aws_sdk_cloudwatchlogs::output::{DescribeLogStreamsOutput, PutLogEventsOutput};
use aws_sdk_cloudwatchlogs::types::SdkError;
use aws_sdk_cloudwatchlogs::Client as CloudwatchLogsClient;
use aws_smithy_http::operation::{Operation, Request};
use futures::{future::BoxFuture, FutureExt};
use indexmap::IndexMap;
use tokio::sync::oneshot;

use crate::sinks::aws_cloudwatch_logs::service::{CloudwatchError, SmithyClient};

pub struct CloudwatchFuture {
    client: Client,
    state: State,
    create_missing_group: bool,
    create_missing_stream: bool,
    events: Vec<Vec<InputLogEvent>>,
    token_tx: Option<oneshot::Sender<Option<String>>>,
}

struct Client {
    client: CloudwatchLogsClient,
    // we store a separate smithy_client to set request headers for PutLogEvents since the regular
    // client cannot set headers
    //
    // https://github.com/awslabs/aws-sdk-rust/issues/537
    smithy_client: SmithyClient,
    stream_name: String,
    group_name: String,
    headers: IndexMap<String, String>,
}

type ClientResult<T, E> = BoxFuture<'static, Result<T, SdkError<E>>>;

enum State {
    CreateGroup(ClientResult<(), CreateLogGroupError>),
    CreateStream(ClientResult<(), CreateLogStreamError>),
    DescribeStream(ClientResult<DescribeLogStreamsOutput, DescribeLogStreamsError>),
    Put(ClientResult<PutLogEventsOutput, PutLogEventsError>),
}

impl CloudwatchFuture {
    /// Panics if events.is_empty()
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        client: CloudwatchLogsClient,
        smithy_client: SmithyClient,
        headers: IndexMap<String, String>,
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
            smithy_client,
            stream_name,
            group_name,
            headers,
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
                        Err(err) => {
                            if let SdkError::ServiceError(inner) = &err {
                                if let DescribeLogStreamsErrorKind::ResourceNotFoundException(_) =
                                    inner.err().kind
                                {
                                    if self.create_missing_group {
                                        info!("Log group provided does not exist; creating a new one.");

                                        self.state =
                                            State::CreateGroup(self.client.create_log_group());
                                        continue;
                                    }
                                }
                            }
                            return Poll::Ready(Err(CloudwatchError::Describe(err)));
                        }
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
                        Err(err) => {
                            let resource_already_exists = match &err {
                                SdkError::ServiceError(inner) => matches!(
                                    inner.err().kind,
                                    CreateLogGroupErrorKind::ResourceAlreadyExistsException(_)
                                ),
                                _ => false,
                            };
                            if !resource_already_exists {
                                return Poll::Ready(Err(CloudwatchError::CreateGroup(err)));
                            }
                        }
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
                        Err(err) => {
                            let resource_already_exists = match &err {
                                SdkError::ServiceError(inner) => matches!(
                                    inner.err().kind,
                                    CreateLogStreamErrorKind::ResourceAlreadyExistsException(_)
                                ),
                                _ => false,
                            };
                            if !resource_already_exists {
                                return Poll::Ready(Err(CloudwatchError::CreateStream(err)));
                            }
                        }
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
    ) -> ClientResult<PutLogEventsOutput, PutLogEventsError> {
        let client = std::sync::Arc::clone(&self.smithy_client);
        let cw_client = self.client.clone();
        let group_name = self.group_name.clone();
        let stream_name = self.stream_name.clone();
        let headers = self.headers.clone();
        Box::pin(async move {
            // #12760 this is a relatively convoluted way of changing the headers of a request
            // about to be sent. https://github.com/awslabs/aws-sdk-rust/issues/537 should
            // eventually make this better.
            let op = PutLogEvents::builder()
                .set_log_events(Some(log_events))
                .set_sequence_token(sequence_token)
                .log_group_name(group_name)
                .log_stream_name(stream_name)
                .build()
                .map_err(SdkError::construction_failure)?
                .make_operation(cw_client.conf())
                .await
                .map_err(SdkError::construction_failure)?;

            let (req, parts) = op.into_request_response();
            let (mut body, props) = req.into_parts();
            for (header, value) in headers.iter() {
                let owned_header = header.clone();
                let owned_value = value.clone();
                body.headers_mut().insert(
                    http::header::HeaderName::from_bytes(owned_header.as_bytes())
                        .map_err(SdkError::construction_failure)?,
                    http::HeaderValue::from_str(owned_value.as_str())
                        .map_err(SdkError::construction_failure)?,
                );
            }
            client
                .call(Operation::from_parts(
                    Request::from_parts(body, props),
                    parts,
                ))
                .await
        })
    }

    pub fn describe_stream(
        &self,
    ) -> ClientResult<DescribeLogStreamsOutput, DescribeLogStreamsError> {
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        let stream_name = self.stream_name.clone();
        Box::pin(async move {
            client
                .describe_log_streams()
                .limit(1)
                .log_group_name(group_name)
                .log_stream_name_prefix(stream_name)
                .send()
                .await
        })
    }

    pub fn create_log_group(&self) -> ClientResult<(), CreateLogGroupError> {
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        Box::pin(async move {
            client
                .create_log_group()
                .log_group_name(group_name)
                .send()
                .await?;
            Ok(())
        })
    }

    pub fn create_log_stream(&self) -> ClientResult<(), CreateLogStreamError> {
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        let stream_name = self.stream_name.clone();
        Box::pin(async move {
            client
                .create_log_stream()
                .log_group_name(group_name)
                .log_stream_name(stream_name)
                .send()
                .await?;
            Ok(())
        })
    }
}
