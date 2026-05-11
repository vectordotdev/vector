//! The main Zerobus sink implementation.

use std::num::NonZeroUsize;

use futures::StreamExt;
use futures::stream::BoxStream;

use vector_lib::finalization::Finalizable;

use crate::sinks::prelude::*;
use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::{RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings};

use super::service::{ZerobusRequest, ZerobusRetryLogic, ZerobusService};

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

            input
                .batched(self.batch_settings.as_byte_size_config())
                .map(|mut events| {
                    let finalizers = events.take_finalizers();
                    let metadata =
                        RequestMetadataBuilder::from_events(&events).with_request_size(NonZeroUsize::MIN);
                    ZerobusRequest {
                        events,
                        metadata,
                        finalizers,
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
