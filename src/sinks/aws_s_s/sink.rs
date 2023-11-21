use super::{client::Client, request_builder::SSRequestBuilder, service::SSService};
use crate::sinks::aws_s_s::retry::SSRetryLogic;
use crate::sinks::prelude::*;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SqsSinkDefaultBatchSettings;

impl SinkBatchSettings for SqsSinkDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1);
    const MAX_BYTES: Option<usize> = Some(262_144);
    const TIMEOUT_SECS: f64 = 1.0;
}

#[derive(Clone)]
pub(super) struct SSSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    request_builder: SSRequestBuilder,
    service: SSService<C, E>,
    request: TowerRequestConfig,
}

impl<C, E> SSSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    pub(super) fn new(
        request_builder: SSRequestBuilder,
        request: TowerRequestConfig,
        publisher: C,
    ) -> crate::Result<Self> {
        Ok(SSSink {
            request_builder,
            service: SSService::new(publisher),
            request,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request = self.request.into_settings();
        let retry_logic: SSRetryLogic<E> = super::retry::SSRetryLogic::new();
        let service = tower::ServiceBuilder::new()
            .settings(request, retry_logic)
            .service(self.service);

        input
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
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
impl<C, E> StreamSink<Event> for SSSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
