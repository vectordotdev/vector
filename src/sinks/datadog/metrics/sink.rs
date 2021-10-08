use async_trait::async_trait;
use futures_util::stream::BoxStream;
use http::Uri;
use vector_core::{event::Event, sink::StreamSink};

use crate::{config::SinkContext, sinks::util::Compression};

use super::config::DatadogMetricsEndpoint;

pub struct DatadogMetricsSink<S> {
    service: S,
    metric_endpoints: Vec<(DatadogMetricsEndpoint, Uri)>,
    compression: Compression,
}

impl<S> DatadogMetricsSink<S> {
    pub fn new(
        _cx: SinkContext,
        service: S,
        metric_endpoints: Vec<(DatadogMetricsEndpoint, Uri)>,
        compression: Compression,
    ) -> Self {
        DatadogMetricsSink {
            service,
            metric_endpoints,
            compression,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        todo!()
    }
}

#[async_trait]
impl<S> StreamSink for DatadogMetricsSink<S> {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
