use std::fmt;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::{future, stream::BoxStream, StreamExt};
use tower::Service;
use vector_core::{
    buffers::{Ackable, Acker},
    partition::Partitioner,
    sink::StreamSink,
    stream::{BatcherSettings, DriverResponse},
};

use crate::{
    event::{Event, EventFinalizers, Finalizable},
    sinks::{
        aws_cloudwatch_logs::{
            request_builder::{CloudwatchRequest, CloudwatchRequestBuilder},
            CloudwatchKey,
        },
        util::SinkBuilderExt,
    },
};

pub struct CloudwatchSink<S> {
    pub batcher_settings: BatcherSettings,
    pub(super) request_builder: CloudwatchRequestBuilder,
    pub acker: Acker,
    pub service: S,
}

impl<S> CloudwatchSink<S>
where
    S: Service<BatchCloudwatchRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut request_builder = self.request_builder;
        let batcher_settings = self.batcher_settings;
        let service = self.service;
        let acker = self.acker;

        input
            .filter_map(|event| future::ready(request_builder.build(event)))
            .filter(|req| {
                let now = Utc::now();
                let start = (now - Duration::days(14) + Duration::minutes(5)).timestamp_millis();
                let end = (now + Duration::hours(2)).timestamp_millis();
                let age_range = start..end;
                future::ready(age_range.contains(&req.timestamp))
            })
            .batched_partitioned(CloudwatchParititoner, batcher_settings)
            .map(|(key, events)| BatchCloudwatchRequest { key, events })
            .into_driver(service, acker)
            .run()
            .await
    }
}

#[derive(Clone)]
pub struct BatchCloudwatchRequest {
    pub key: CloudwatchKey,
    pub events: Vec<CloudwatchRequest>,
}

impl Ackable for BatchCloudwatchRequest {
    fn ack_size(&self) -> usize {
        self.events.len()
    }
}

impl Finalizable for BatchCloudwatchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.events.take_finalizers()
    }
}

struct CloudwatchParititoner;

impl Partitioner for CloudwatchParititoner {
    type Item = CloudwatchRequest;
    type Key = CloudwatchKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.key.clone()
    }
}

#[async_trait]
impl<S> StreamSink<Event> for CloudwatchSink<S>
where
    S: Service<BatchCloudwatchRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
