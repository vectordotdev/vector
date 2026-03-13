//! The main Zerobus sink implementation.

use std::num::NonZeroUsize;
use std::sync::Arc;

use futures::stream::BoxStream;

use vector_lib::codecs::encoding::{BatchEncoder, BatchOutput};
use vector_lib::event::EventStatus;
use vector_lib::finalization::Finalizable;

use crate::sinks::prelude::*;
use crate::sinks::util::metadata::RequestMetadataBuilder;
use crate::sinks::util::request_builder::default_request_builder_concurrency_limit;
use crate::sinks::util::{RealtimeSizeBasedDefaultBatchSettings, TowerRequestSettings};

use super::service::{ZerobusPayload, ZerobusRequest, ZerobusRetryLogic, ZerobusService};

/// The main Zerobus sink.
pub struct ZerobusSink {
    service: ZerobusService,
    request_limits: TowerRequestSettings,
    batch_settings: BatcherSettings,
    encoder: BatchEncoder,
}

impl ZerobusSink {
    pub fn new(
        service: ZerobusService,
        request_limits: TowerRequestSettings,
        batch_config: BatchConfig<RealtimeSizeBasedDefaultBatchSettings>,
        encoder: BatchEncoder,
    ) -> Result<Self, crate::Error> {
        let batch_settings = batch_config.into_batcher_settings()?;

        Ok(Self {
            service,
            request_limits,
            batch_settings,
            encoder,
        })
    }

    fn encode_batch(
        encoder: &BatchEncoder,
        mut events: Vec<Event>,
    ) -> Result<ZerobusRequest, String> {
        let finalizers = events.take_finalizers();
        let metadata_builder = RequestMetadataBuilder::from_events(&events);

        let batch_output = match encoder.encode_batch(&events) {
            Ok(output) => output,
            Err(e) => {
                finalizers.update_status(EventStatus::Rejected);
                return Err(format!("Failed to encode batch: {}", e));
            }
        };

        let (payload, byte_size) = match batch_output {
            BatchOutput::Records(records) => {
                let size = records.iter().map(|r| r.len()).sum::<usize>();
                (ZerobusPayload::Records(records), size)
            }
            #[cfg(feature = "codecs-arrow")]
            BatchOutput::Arrow(record_batch) => {
                let size = record_batch.get_array_memory_size();
                (ZerobusPayload::Arrow(record_batch), size)
            }
            #[allow(unreachable_patterns)]
            _ => {
                finalizers.update_status(EventStatus::Rejected);
                return Err("Unexpected batch output type".to_string());
            }
        };

        let request_size = NonZeroUsize::new(byte_size).unwrap_or(NonZeroUsize::MIN);
        let metadata = metadata_builder.with_request_size(request_size);

        Ok(ZerobusRequest {
            payload,
            metadata,
            finalizers,
        })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let encoder = Arc::new(self.encoder.clone());

        let result = {
            let tower_service = ServiceBuilder::new()
                .settings(self.request_limits, ZerobusRetryLogic)
                .service(self.service.clone());

            input
                .batched(self.batch_settings.as_byte_size_config())
                .concurrent_map(default_request_builder_concurrency_limit(), move |events| {
                    let encoder = Arc::clone(&encoder);
                    Box::pin(async move { Self::encode_batch(&encoder, events) })
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
