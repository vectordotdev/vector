use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    task::{Context, Poll, ready},
};

use aws_sdk_cloudwatchlogs::{
    Client as CloudwatchLogsClient,
    operation::{
        create_log_group::CreateLogGroupError,
        create_log_stream::CreateLogStreamError,
        describe_log_streams::{DescribeLogStreamsError, DescribeLogStreamsOutput},
        put_log_events::{PutLogEventsError, PutLogEventsOutput},
        put_retention_policy::PutRetentionPolicyError,
    },
    types::InputLogEvent,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::{FutureExt, future::BoxFuture};
use http::{HeaderValue, header::HeaderName};
use indexmap::IndexMap;

use crate::sinks::aws_cloudwatch_logs::{config::Retention, service::CloudwatchError};

pub struct CloudwatchFuture {
    client: Client,
    state: State,
    create_missing_group: bool,
    create_missing_stream: bool,
    retention_enabled: bool,
    // Batches still waiting to be sent after `current`.
    events: Vec<Vec<InputLogEvent>>,
    // The batch currently in flight. Retained so it can be resent after a
    // missing log group/stream is created.
    current: Vec<InputLogEvent>,
    // Set once we've dropped into the describe/create path from a
    // `ResourceNotFoundException`, so a second one is a hard error rather than
    // an infinite resolve loop.
    resolving: bool,
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

        // Since January 2023, CloudWatch Logs no longer requires a sequence
        // token on `PutLogEvents` and never returns `InvalidSequenceToken`, so
        // we write directly. The old sink called `DescribeLogStreams` before
        // every batch (and again on every retry) just to fetch that token,
        // which is throttled account+region-wide at a low default quota
        // (25 TPS) and starved the sink under load. We now only fall back to
        // `DescribeLogStreams` when `PutLogEvents` reports the group/stream is
        // missing, i.e. once per new stream, not once per batch.
        // https://aws.amazon.com/about-aws/whats-new/2023/01/amazon-cloudwatch-logs-log-stream-transaction-quota-sequencetoken-requirement/
        let current = events.pop().expect("No Events to send");
        let state = State::Put(client.put_logs(current.clone()));

        let retention_enabled = retention.enabled;

        Self {
            client,
            events,
            current,
            resolving: false,
            state,
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
                State::Put(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        Ok(_response) => {
                            if let Some(events) = self.events.pop() {
                                self.current = events;
                                debug!(message = "Putting logs.");
                                self.state =
                                    State::Put(self.client.put_logs(self.current.clone()));
                            } else {
                                info!(message = "Putting logs was successful.");
                                return Poll::Ready(Ok(()));
                            }
                        }
                        Err(err) => {
                            // The group or stream is missing. Resolve it once
                            // with a describe (which distinguishes a missing
                            // group from a missing stream), then replay.
                            if !self.resolving && is_resource_not_found(&err) {
                                self.resolving = true;
                                self.state = State::DescribeStream(self.client.describe_stream());
                                continue;
                            }
                            return Poll::Ready(Err(CloudwatchError::Put(err)));
                        }
                    }
                }

                State::DescribeStream(fut) => {
                    let response = match ready!(fut.poll_unpin(cx)) {
                        Ok(response) => response,
                        Err(err) => {
                            if let SdkError::ServiceError(inner) = &err
                                && matches!(
                                    inner.err(),
                                    DescribeLogStreamsError::ResourceNotFoundException(_)
                                )
                                && self.create_missing_group
                            {
                                info!("Log group provided does not exist; creating a new one.");

                                self.state = State::CreateGroup(self.client.create_log_group());
                                continue;
                            }
                            return Poll::Ready(Err(CloudwatchError::DescribeLogStreams(err)));
                        }
                    };

                    let stream_name = &self.client.stream_name;

                    if response
                        .log_streams
                        .ok_or(CloudwatchError::NoStreamsFound)?
                        .into_iter()
                        .any(|log_stream| log_stream.log_stream_name == Some(stream_name.clone()))
                    {
                        debug!(message = "Stream found.", stream = ?stream_name);
                        self.state = State::Put(self.client.put_logs(self.current.clone()));
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
                        self.state =
                            State::PutRetentionPolicy(self.client.put_retention_policy());
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

                    // No sequence token needed, so replay the batch straight away.
                    self.state = State::Put(self.client.put_logs(self.current.clone()));
                }

                State::PutRetentionPolicy(fut) => {
                    match ready!(fut.poll_unpin(cx)) {
                        Ok(_) => {}
                        Err(error) => {
                            return Poll::Ready(Err(CloudwatchError::PutRetentionPolicy(error)));
                        }
                    }

                    info!(message = "Retention policy updated for stream.", name = %self.client.stream_name);

                    self.state = State::CreateStream(self.client.create_log_stream());
                }
            }
        }
    }
}

fn is_resource_not_found(err: &SdkError<PutLogEventsError, HttpResponse>) -> bool {
    matches!(
        err,
        SdkError::ServiceError(inner)
            if matches!(inner.err(), PutLogEventsError::ResourceNotFoundException(_))
    )
}

impl Client {
    pub fn put_logs(
        &self,
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
