use crate::config::LogSchema;
use crate::event::{Event, EventFinalizers, Finalizable, LogEvent, Value};
use crate::internal_events::TemplateRenderingFailed;
use crate::sinks::aws_cloudwatch_logs::request_builder::{
    CloudwatchRequest, CloudwatchRequestBuilder,
};
use crate::sinks::aws_cloudwatch_logs::retry::CloudwatchRetryLogic;
use crate::sinks::aws_cloudwatch_logs::service::{CloudwatchLogsPartitionSvc, CloudwatchResponse};
use crate::sinks::aws_cloudwatch_logs::{CloudwatchKey, CloudwatchLogsError};
use crate::sinks::util::encoding::{
    Encoder, EncodingConfig, EncodingConfiguration, StandardEncodings,
};
use crate::sinks::util::processed_event::ProcessedEvent;
use crate::sinks::util::service::Svc;
use crate::sinks::util::{EncodedEvent, SinkBuilderExt};
use crate::template::Template;
use async_graphql::futures_util::stream::BoxStream;
use async_trait::async_trait;
use chrono::Utc;
use futures::future;
use futures::FutureExt;
use futures::StreamExt;
use rusoto_logs::InputLogEvent;
use std::num::NonZeroUsize;
use vector_core::buffers::{Ackable, Acker};
use vector_core::partition::Partitioner;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;
use vector_core::ByteSizeOf;

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
            .filter_map(|mut event| {
                future::ready(match request_builder.build(event) {
                    Ok(maybe_req) => maybe_req.map(|x| Ok(x)),
                    Err(err) => Some(Err(err)),
                })
            })
            .filter_map(|request| async move {
                match request {
                    Err(e) => {
                        error!("Failed to build Cloudwatch Logs request: {:?}.", e);
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .batched_partitioned(CloudwatchParititoner, batcher_settings)
            .map(|(key, events)| BatchCloudwatchRequest { key, events })
            .into_driver(service, acker)
            .run()
            .await;

        Ok(())
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

fn test(sink: CloudwatchSink, input: BoxStream<'_, Event>) {
    let future = Box::new(sink).run_inner(input).boxed();
}

#[async_trait]
impl StreamSink for CloudwatchSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
