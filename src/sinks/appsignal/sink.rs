use futures_util::future::ready;

use crate::sinks::{prelude::*, util::buffer::metrics::MetricNormalizer};

use super::{
    encoder::AppsignalEncoder,
    normalizer::AppsignalMetricsNormalizer,
    request_builder::{AppsignalRequest, AppsignalRequestBuilder},
};

pub(super) struct AppsignalSink<S> {
    pub(super) service: S,
    pub(super) compression: Compression,
    pub(super) transformer: Transformer,
    pub(super) batch_settings: BatcherSettings,
}

impl<S> AppsignalSink<S>
where
    S: Service<AppsignalRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    pub(super) async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let service = ServiceBuilder::new().service(self.service);
        let mut normalizer = MetricNormalizer::<AppsignalMetricsNormalizer>::default();

        input
            .filter_map(move |event| {
                ready(if let Event::Metric(metric) = event {
                    normalizer.normalize(metric).map(Event::Metric)
                } else {
                    Some(event)
                })
            })
            .batched(self.batch_settings.as_byte_size_config())
            .request_builder(
                default_request_builder_concurrency_limit(),
                AppsignalRequestBuilder {
                    compression: self.compression,
                    encoder: AppsignalEncoder {
                        transformer: self.transformer.clone(),
                    },
                },
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl<S> StreamSink<Event> for AppsignalSink<S>
where
    S: Service<AppsignalRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: std::fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(
        self: Box<Self>,
        input: futures_util::stream::BoxStream<'_, Event>,
    ) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
