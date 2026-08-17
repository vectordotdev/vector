use super::{client::Client, request_builder::SSRequestBuilder, service::SSService};
use crate::sinks::{aws_s_s::retry::SSRetryLogic, prelude::*};

#[derive(Clone)]
pub(super) struct SSSink<C, E>
where
    C: Client<E> + Clone + Send + Sync + 'static,
    E: std::fmt::Debug + std::fmt::Display + std::error::Error + Sync + Send + 'static,
{
    request_builder: SSRequestBuilder,
    service: SSService<C, E>,
    request: TowerRequestConfig,
    /// The AWS region string for metric labels.
    region: String,
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
        region: String,
    ) -> crate::Result<Self> {
        Ok(SSSink {
            request_builder,
            service: SSService::new(publisher),
            request,
            region,
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
            .protocol("https")
            .label("region", self.region)
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
