use std::num::NonZeroUsize;

use aws_sdk_sqs::Client as SqsClient;
use futures::stream::BoxStream;
use futures_util::StreamExt;
use vector_core::sink::StreamSink;

use super::{config::SqsSinkConfig, request_builder::SqsRequestBuilder, service::SqsService};
use crate::internal_events::SinkRequestBuildError;
use crate::{
    event::Event,
    sinks::util::{
        builder::SinkBuilderExt, ServiceBuilderExt, SinkBatchSettings, TowerRequestConfig,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SqsSinkDefaultBatchSettings;

impl SinkBatchSettings for SqsSinkDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = Some(262_144);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone)]
pub(crate) struct SqsSink {
    request_builder: SqsRequestBuilder,
    service: SqsService,
    request: TowerRequestConfig,
}

impl SqsSink {
    pub fn new(config: SqsSinkConfig, client: SqsClient) -> crate::Result<Self> {
        let request = config.request;
        Ok(SqsSink {
            request_builder: SqsRequestBuilder::new(config)?,
            service: SqsService::new(client),
            request,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self
            .request
            .unwrap_with(&TowerRequestConfig::default().timeout_secs(30));
        let request_builder_concurrency_limit = NonZeroUsize::new(50);
        let service = tower::ServiceBuilder::new()
            .settings(request, super::retry::SqsRetryLogic)
            .service(self.service);

        input
            .request_builder(request_builder_concurrency_limit, self.request_builder)
            .filter_map(|req| async move {
                req.map_err(|error| {
                    emit!(SinkRequestBuildError { error });
                })
                .ok()
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for SqsSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
