use async_trait::async_trait;
use chrono::{Duration, Utc};
use futures::{future, stream::BoxStream, StreamExt};
use vector_core::{
    buffers::{Ackable, Acker},
    partition::Partitioner,
    sink::StreamSink,
    stream::BatcherSettings,
};

use crate::{
    event::{Event, EventFinalizers, Finalizable},
    sinks::{
        aws_cloudwatch_logs::{
            request_builder::{CloudwatchRequest, CloudwatchRequestBuilder},
            retry::CloudwatchRetryLogic,
            service::{CloudwatchLogsPartitionSvc, CloudwatchResponse},
            CloudwatchKey,
        },
        util::{service::Svc, SinkBuilderExt},
    },
};

pub struct CloudwatchSink {
    pub batcher_settings: BatcherSettings,
    pub request_builder: CloudwatchRequestBuilder,
    pub acker: Acker,
    pub service: Svc<CloudwatchLogsPartitionSvc, CloudwatchRetryLogic<CloudwatchResponse>>,
}

impl CloudwatchSink {
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = self.request_builder;
        let batcher_settings = self.batcher_settings;
        let service = self.service;
        let acker = self.acker;

        input
            .filter_map(|event| future::ready(request_builder.build(event).transpose()))
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Cloudwatch Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
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
impl StreamSink<Event> for CloudwatchSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
