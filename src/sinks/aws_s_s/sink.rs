use std::num::NonZeroUsize;

use futures::stream::BoxStream;
use futures_util::StreamExt;
use vector_core::sink::StreamSink;

use super::{client::Client, request_builder::SqsRequestBuilder, service::SqsService};
use crate::internal_events::SinkRequestBuildError;
use crate::sinks::aws_s_s::config::ConfigWithIds;
use crate::sinks::aws_s_s::retry::SqsRetryLogic;
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
pub struct SqsSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    request_builder: SqsRequestBuilder,
    service: SqsService<C, E>,
    request: TowerRequestConfig,
}

impl<C, E> SqsSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    pub fn new(config: ConfigWithIds, publisher: C) -> crate::Result<Self> {
        let request = config.base_config.request;
        Ok(SqsSink {
            request_builder: SqsRequestBuilder::new(config)?,
            service: SqsService::new(publisher),
            request,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self
            .request
            .unwrap_with(&TowerRequestConfig::default().timeout_secs(30));
        let request_builder_concurrency_limit = NonZeroUsize::new(50);
        let retry_logic: SqsRetryLogic<E> = super::retry::SqsRetryLogic::new();
        let service = tower::ServiceBuilder::new()
            .settings(request, retry_logic)
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
impl<C, E> StreamSink<Event> for SqsSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
