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
        put_log_events::{PutLogEventsError, PutLogEventsOutput},
        put_retention_policy::PutRetentionPolicyError,
    },
    types::InputLogEvent,
};
use aws_smithy_runtime_api::client::{orchestrator::HttpResponse, result::SdkError};
use futures::{FutureExt, future::BoxFuture};
use http::{HeaderValue, header::HeaderName};
use indexmap::IndexMap;
use tokio::sync::oneshot;

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
    // missing log group/stream is created: `PutLogEvents` returns
    // `ResourceNotFoundException` when the stream does not exist yet, and we
    // need the events to replay once it does.
    current: Vec<InputLogEvent>,
    // Set once we have created (or tried to create) the group/stream in
    // response to a `ResourceNotFoundException`, so a second failure is a hard
    // error rather than an infinite create loop.
    created_missing: bool,
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
        // Kept for backwards compatibility with the caller; the sequence token
        // is no longer used (see below) and is always ignored.
        _token: Option<String>,
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

        // Since January 2023, CloudWatch Logs no longer requires a sequence
        // token on `PutLogEvents` and never returns `InvalidSequenceToken`, so
        // we write directly instead of first calling `DescribeLogStreams` to
        // fetch a token. That describe call ran once per batch (and again on
        // every retry), and is account+region-wide throttled at a low default
        // quota (25 TPS for `DescribeLogStreams`), so on large fleets it became
        // the bottleneck and starved the sink. Writing straight to
        // `PutLogEvents` removes that dependency entirely.
        // https://aws.amazon.com/about-aws/whats-new/2023/01/amazon-cloudwatch-logs-log-stream-transaction-quota-sequencetoken-requirement/
        let current = events.pop().expect("No Events to send");
        let state = State::Put(client.put_logs(current.clone()));

        let retention_enabled = retention.enabled;

        Self {
            client,
            events,
            current,
            created_missing: false,
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

                                self.token_tx
                                    .take()
                                    .expect("Put was polled after finishing.")
                                    .send(None)
                                    .expect("CloudwatchLogsSvc was dropped unexpectedly");

                                return Poll::Ready(Ok(()));
                            }
                        }
                        Err(err) => {
                            // The stream (or its group) does not exist yet.
                            // Create it once, then replay the same batch.
                            if !self.created_missing
                                && is_resource_not_found(&err)
                                && (self.create_missing_group || self.create_missing_stream)
                            {
                                self.created_missing = true;
                                if self.create_missing_group {
                                    info!(
                                        "Log group provided does not exist; creating a new one."
                                    );
                                    self.state =
                                        State::CreateGroup(self.client.create_log_group());
                                } else {
                                    info!("Provided stream does not exist; creating a new one.");
                                    self.state =
                                        State::CreateStream(self.client.create_log_stream());
                                }
                                continue;
                            }
                            return Poll::Ready(Err(CloudwatchError::Put(err)));
                        }
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

                    // A newly created group never has any streams, so create
                    // one regardless of `create_missing_stream`.
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

                    // Replay the batch that hit the missing group/stream.
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
