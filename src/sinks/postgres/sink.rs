use super::service::{PostgresRequest, PostgresRetryLogic, PostgresService};
use crate::sinks::prelude::*;

pub struct PostgresSink {
    service: Svc<PostgresService, PostgresRetryLogic>,
    batch_settings: BatcherSettings,
}

impl PostgresSink {
    pub const fn new(
        service: Svc<PostgresService, PostgresRetryLogic>,
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
                match PostgresRequest::try_from(events) {
                    Ok(request) => Some(request),
                    Err(e) => {
                        warn!(
                            message = "Error creating postgres sink's request.",
                            error = %e,
                            internal_log_rate_limit=true
                        );
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
impl StreamSink<Event> for PostgresSink {
    async fn run(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
