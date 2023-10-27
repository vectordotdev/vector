use std::fmt;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::{future, stream::BoxStream, StreamExt};
use tower::Service;
use vector_lib::request_metadata::{MetaDescriptive, RequestMetadata};
use vector_lib::stream::{BatcherSettings, DriverResponse};
use vector_lib::{partition::Partitioner, sink::StreamSink};

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

        input
            .filter_map(|event| future::ready(request_builder.build(event)))
            .filter(|req| {
                let now = Utc::now();
                let start = (now - Duration::days(14) + Duration::minutes(5)).timestamp_millis();
                let end = (now + Duration::hours(2)).timestamp_millis();
                let age_range = start..end;
                future::ready(age_range.contains(&req.timestamp))
            })
            .batched_partitioned(CloudwatchPartitioner, || {
                batcher_settings.as_byte_size_config()
            })
            .map(|(key, events)| {
                let metadata = RequestMetadata::from_batch(
                    events.iter().map(|req| req.get_metadata().clone()),
                );

                BatchCloudwatchRequest {
                    key,
                    events,
                    metadata,
                }
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[derive(Clone)]
pub struct BatchCloudwatchRequest {
    pub key: CloudwatchKey,
    pub events: Vec<CloudwatchRequest>,
    metadata: RequestMetadata,
}

impl Finalizable for BatchCloudwatchRequest {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.events.take_finalizers()
    }
}

impl MetaDescriptive for BatchCloudwatchRequest {
    fn get_metadata(&self) -> &RequestMetadata {
        &self.metadata
    }

    fn metadata_mut(&mut self) -> &mut RequestMetadata {
        &mut self.metadata
    }
}

struct CloudwatchPartitioner;

impl Partitioner for CloudwatchPartitioner {
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
