use aws_sdk_cloudwatchlogs::{
    operation::{
        create_log_group::CreateLogGroupError,
        create_log_stream::CreateLogStreamError,
        describe_log_streams::{DescribeLogStreamsError, DescribeLogStreamsOutput},
        put_log_events::{PutLogEventsError, PutLogEventsOutput},
        put_retention_policy::PutRetentionPolicyError,
    },
    types::InputLogEvent,
    Client as CloudwatchLogsClient,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::{future::BoxFuture, FutureExt};
use http::{header::HeaderName, HeaderValue};
use indexmap::IndexMap;
use std::collections::HashMap;
use std::{
    future::Future,
    pin::Pin,
    task::{ready, Context, Poll},
};
use tokio::sync::oneshot;

use crate::sinks::aws_cloudwatch_logs::config::Retention;
use crate::sinks::aws_cloudwatch_logs::service::CloudwatchError;

pub struct CloudwatchFuture {
    client: Client,
    state: State,
    create_missing_group: bool,
    create_missing_stream: bool,
    retention_enabled: bool,
    events: Vec<Vec<InputLogEvent>>,
    token_tx: Option<oneshot::Sender<Option<String>>>,
}

struct Client {
    client: CloudwatchLogsClient,
    stream_name: String,
    group_name: String,
    headers: IndexMap<HeaderName, HeaderValue>,
    retention_days: u32,
    kms_key: Option<String>,
    tags: Option<HashMap<String, String>>,
}

type ClientResult<T, E> = BoxFuture<'static, Result<T, SdkError<E, HttpResponse>>>;

enum State {
    CreateGroup(ClientResult<(), CreateLogGroupError>),
    CreateStream(ClientResult<(), CreateLogStreamError>),
    DescribeStream(ClientResult<DescribeLogStreamsOutput, DescribeLogStreamsError>),
    Put(ClientResult<PutLogEventsOutput, PutLogEventsError>),
    PutRetentionPolicy(ClientResult<(), PutRetentionPolicyError>),
}

impl CloudwatchFuture {
    /// Panics if events.is_empty()
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        client: CloudwatchLogsClient,
        headers: IndexMap<HeaderName, HeaderValue>,
        stream_name: String,
        group_name: String,
        create_missing_group: bool,
        create_missing_stream: bool,
        retention: Retention,
        kms_key: Option<String>,
        tags: Option<HashMap<String, String>>,
        mut events: Vec<Vec<InputLogEvent>>,
        token: Option<String>,
        token_tx: oneshot::Sender<Option<String>>,
    ) -> Self {
        let retention_days = retention.days;
        let client = Client {
            client,
            stream_name,
            group_name,
            headers,
            retention_days,
            kms_key,
            tags,
        };

        let state = if let Some(token) = token {
            State::Put(client.put_logs(Some(token), events.pop().expect("No Events to send")))
        } else {
            State::DescribeStream(client.describe_stream())
        };

        let retention_enabled = retention.enabled;

        Self {
            client,
            events,
            state,
            token_tx: Some(token_tx),
            create_missing_group,
            create_missing_stream,
            retention_enabled,
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
                                if matches!(
                                    inner.err(),
                                    DescribeLogStreamsError::ResourceNotFoundException(_)
                                ) && self.create_missing_group
                                {
                                    info!("Log group provided does not exist; creating a new one.");

                                    self.state = State::CreateGroup(self.client.create_log_group());
                                    continue;
                                }
                            }
                            return Poll::Ready(Err(CloudwatchError::DescribeLogStreams(err)));
                        }
                    };

                    let stream_name = &self.client.stream_name;

                    if let Some(stream) = response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .find(|log_stream| log_stream.log_stream_name == Some(stream_name.clone()))
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
                                    inner.err(),
                                    CreateLogGroupError::ResourceAlreadyExistsException(_)
                                ),
                                _ => false,
                            };
                            if !resource_already_exists {
                                return Poll::Ready(Err(CloudwatchError::CreateGroup(err)));
                            }
                        }
                    };

                    info!(message = "Group created.", name = %self.client.group_name);

                    if self.retention_enabled {
                        self.state = State::PutRetentionPolicy(self.client.put_retention_policy());
                        continue;
                    }

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
                                    inner.err(),
                                    CreateLogStreamError::ResourceAlreadyExistsException(_)
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

                State::PutRetentionPolicy(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        Ok(_) => {}
                        Err(error) => {
                            return Poll::Ready(Err(CloudwatchError::PutRetentionPolicy(error)))
                        }
                    }

                    info!(message = "Retention policy updated for stream.", name = %self.client.stream_name);

                    self.state = State::CreateStream(self.client.create_log_stream());
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
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        let stream_name = self.stream_name.clone();
        let headers = self.headers.clone();

        Box::pin(async move {
            client
                .put_log_events()
                .set_log_events(Some(log_events))
                .set_sequence_token(sequence_token)
                .log_group_name(group_name)
                .log_stream_name(stream_name)
                .customize()
                .mutate_request(move |req| {
                    for (header, value) in headers.iter() {
                        req.headers_mut().insert(header.clone(), value.clone());
                    }
                })
                .send()
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
                .log_group_name(group_name)
                .log_stream_name_prefix(stream_name)
                .send()
                .await
        })
    }

    pub fn create_log_group(&self) -> ClientResult<(), CreateLogGroupError> {
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        let kms_key = self.kms_key.clone();
        let tags = self.tags.clone();
        Box::pin(async move {
            client
                .create_log_group()
                .log_group_name(group_name)
                .set_kms_key_id(kms_key)
                .set_tags(tags)
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

    pub fn put_retention_policy(&self) -> ClientResult<(), PutRetentionPolicyError> {
        let client = self.client.clone();
        let group_name = self.group_name.clone();
        let retention_days = self.retention_days;
        Box::pin(async move {
            client
                .put_retention_policy()
                .log_group_name(group_name)
                .retention_in_days(retention_days.try_into().unwrap())
                .send()
                .await?;
            Ok(())
        })
    }
}
