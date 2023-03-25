use std::num::NonZeroUsize;

use futures_util::{stream::BoxStream, StreamExt};
use vector_core::event::Event;
use vector_core::sink::StreamSink;
use vector_core::stream::BatcherSettings;

use crate::{
    internal_events::SinkRequestBuildError,
    sinks::util::{service::Svc, SinkBuilderExt},
};

use super::request_builder::DatabendRequestBuilder;
use super::service::{DatabendRetryLogic, DatabendService};

pub struct DatabendSink {
    batch_settings: BatcherSettings,
    request_builder: DatabendRequestBuilder,
    service: Svc<DatabendService, DatabendRetryLogic>,
}

impl DatabendSink {
    pub(super) const fn new(
        batch_settings: BatcherSettings,
        request_builder: DatabendRequestBuilder,
        service: Svc<DatabendService, DatabendRetryLogic>,
    ) -> Self {
        Self {
            batch_settings,
            request_builder,
            service,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let builder_limit = NonZeroUsize::new(64);
        input
            .batched(self.batch_settings.into_byte_size_config())
            .request_builder(builder_limit, self.request_builder)
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(req) => Some(req),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for DatabendSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
