//! The main Zerobus sink implementation.

use futures::stream::BoxStream;

use vector_lib::event::EventStatus;
use vector_lib::finalization::Finalizable;

use crate::sinks::prelude::*;
use crate::sinks::util::request_builder::default_request_builder_concurrency_limit;
use crate::sinks::util::{RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings};

use super::service::{ZerobusRetryLogic, ZerobusService};

/// The main Zerobus sink.
pub struct ZerobusSink {
    service: ZerobusService,
    request_limits: TowerRequestSettings,
    batch_settings: BatcherSettings,
}

impl ZerobusSink {
    pub fn new(
        service: ZerobusService,
        request_limits: TowerRequestSettings,
        batch_config: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
    ) -> Result<Self, crate::Error> {
        let batch_settings = batch_config.into_batcher_settings()?;

        Ok(Self {
            service,
            request_limits,
            batch_settings,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let result = {
            let tower_service = ServiceBuilder::new()
                .settings(self.request_limits, ZerobusRetryLogic)
                .service(self.service.clone());

            let encoding_service = self.service.clone();
            input
                .batched(self.batch_settings.as_byte_size_config())
                .concurrent_map(default_request_builder_concurrency_limit(), move |mut events| {
                    let service = encoding_service.clone();
                    Box::pin(async move {
                        match service.ensure_schema().await {
                            Ok(schema) => ZerobusService::encode_batch(schema, events),
                            Err(e) => {
                                events.take_finalizers().update_status(EventStatus::Rejected);
                                Err(e)
                            }
                        }
                    })
                })
                .filter_map(|result| async move {
                    match result {
                        Err(error) => {
                            emit!(SinkRequestBuildError { error });
                            None
                        }
                        Ok(req) => Some(req),
                    }
                })
                .into_driver(tower_service)
                .run()
                .await
        };

        self.service.close_stream().await;

        result
    }
}

#[async_trait::async_trait]
impl StreamSink<Event> for ZerobusSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
