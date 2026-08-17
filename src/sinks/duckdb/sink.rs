use super::service::{DuckdbRequest, DuckdbRetryLogic, DuckdbService};
use crate::sinks::prelude::*;

pub struct DuckdbSink {
    service: Svc<DuckdbService, DuckdbRetryLogic>,
    batch_settings: BatcherSettings,
}

impl DuckdbSink {
    pub const fn new(
        service: Svc<DuckdbService, DuckdbRetryLogic>,
        batch_settings: BatcherSettings,
    ) -> Self {
        Self {
            service,
            batch_settings,
        }
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        input
            .batched(self.batch_settings.as_byte_size_config())
            .filter_map(|events| async move {
                match DuckdbRequest::try_from(events) {
                    Ok(request) => Some(request),
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for DuckdbSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
