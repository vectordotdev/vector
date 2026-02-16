use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use azure_messaging_eventhubs::{EventDataBatchOptions, ProducerClient};
use bytes::{Bytes, BytesMut};
use futures_util::StreamExt;
use tokio::time::sleep;
use tokio_util::codec::Encoder as _;
use vector_lib::lookup::lookup_v2::OptionalTargetPath;

use super::config::AzureEventHubsSinkConfig;
use crate::{
    sinks::prelude::*,
    sources::azure_event_hubs::build_credential,
};

pub struct AzureEventHubsSink {
    producer: Arc<ProducerClient>,
    transformer: Transformer,
    encoder: Encoder<()>,
    partition_id_field: Option<OptionalTargetPath>,
    batch_enabled: bool,
    batch_max_events: usize,
    batch_timeout: Duration,
    rate_limit_num: u64,
    rate_limit_duration: Duration,
}

impl AzureEventHubsSink {
    pub async fn new(config: &AzureEventHubsSinkConfig) -> crate::Result<Self> {
        let (namespace, event_hub_name, credential, custom_endpoint) = build_credential(
            config.connection_string.as_ref(),
            config.namespace.as_deref(),
            config.event_hub_name.as_deref(),
        )?;

        let retry_options = azure_messaging_eventhubs::RetryOptions {
            initial_delay: azure_core::time::Duration::milliseconds(config.retry_initial_delay_ms as i64),
            max_delay: azure_core::time::Duration::seconds(30),
            max_retries: config.retry_max_retries,
            max_total_elapsed: azure_core::time::Duration::seconds(config.retry_max_elapsed_secs as i64),
        };

        let mut builder = ProducerClient::builder()
            .with_retry_options(retry_options);
        if let Some(endpoint) = custom_endpoint {
            builder = builder.with_custom_endpoint(endpoint);
        }
        let producer = builder
            .open(&namespace, &event_hub_name, credential)
            .await
            .map_err(|e| format!("Failed to create Event Hubs producer: {e}"))?;

        let transformer = config.encoding.transformer();
        let serializer = config.encoding.build()?;
        let encoder = Encoder::<()>::new(serializer);

        Ok(Self {
            producer: Arc::new(producer),
            transformer,
            encoder,
            partition_id_field: config.partition_id_field.clone(),
            batch_enabled: config.batch_enabled,
            batch_max_events: config.batch_max_events,
            batch_timeout: Duration::from_secs(config.batch_timeout_secs),
            rate_limit_num: config.rate_limit_num,
            rate_limit_duration: Duration::from_secs(config.rate_limit_duration_secs),
        })
    }

    /// Encode an event to bytes and extract optional partition ID.
    fn encode_event(&mut self, mut event: Event) -> Option<(Option<String>, Bytes, EventFinalizers)> {
        let finalizers = event.take_finalizers();

        let partition_id = self.partition_id_field.as_ref().and_then(|field| {
            field.path.as_ref().and_then(|path| {
                if let Event::Log(ref log) = event {
                    log.get(path).map(|v| v.to_string_lossy().into_owned())
                } else {
                    None
                }
            })
        });

        self.transformer.transform(&mut event);

        let mut buf = BytesMut::new();
        if self.encoder.encode(event, &mut buf).is_err() {
            return None;
        }

        Some((partition_id, buf.freeze(), finalizers))
    }

    /// Flush buffered events as EventDataBatch per partition.
    async fn flush_batches(
        &self,
        buffer: &mut Vec<(Option<String>, Bytes, EventFinalizers)>,
    ) -> Result<(), ()> {
        if buffer.is_empty() {
            return Ok(());
        }

        // Group by partition_id
        let mut by_partition: HashMap<Option<String>, Vec<(Bytes, EventFinalizers)>> =
            HashMap::new();
        for (pid, body, fins) in buffer.drain(..) {
            by_partition.entry(pid).or_default().push((body, fins));
        }

        for (partition_id, events) in by_partition {
            let batch_options = EventDataBatchOptions {
                partition_id: partition_id.clone(),
                ..Default::default()
            };

            let mut batch = self
                .producer
                .create_batch(Some(batch_options))
                .await
                .map_err(|e| {
                    emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                        error: format!("Failed to create batch: {e}"),
                    });
                })?;

            let mut all_finalizers = Vec::new();
            let mut total_bytes = 0usize;

            for (body, finalizers) in events {
                let event_data = azure_messaging_eventhubs::models::EventData::builder()
                    .with_body(body.to_vec())
                    .build();

                total_bytes += body.len();
                all_finalizers.push(finalizers);

                match batch.try_add_event_data(event_data.clone(), None) {
                    Ok(true) => {} // added successfully
                    Ok(false) => {
                        // Batch full — send current, create new, add this event
                        self.send_batch_inner(batch).await?;

                        let batch_options = EventDataBatchOptions {
                            partition_id: partition_id.clone(),
                            ..Default::default()
                        };
                        batch = self
                            .producer
                            .create_batch(Some(batch_options))
                            .await
                            .map_err(|e| {
                                emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                                    error: format!("Failed to create batch: {e}"),
                                });
                            })?;

                        if let Err(e) = batch.try_add_event_data(event_data, None) {
                            emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                                error: format!("Event too large for batch: {e}"),
                            });
                        }
                    }
                    Err(e) => {
                        emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                            error: format!("Failed to add event to batch: {e}"),
                        });
                    }
                }
            }

            if !batch.is_empty() {
                self.send_batch_inner(batch).await?;
            }

            // Mark all events in this partition as delivered
            for finalizers in all_finalizers {
                finalizers.update_status(EventStatus::Delivered);
            }

            debug!(
                message = "Batch sent.",
                partition_id = ?partition_id,
                bytes = total_bytes,
            );
        }

        Ok(())
    }

    async fn send_batch_inner(
        &self,
        batch: azure_messaging_eventhubs::EventDataBatch<'_>,
    ) -> Result<(), ()> {
        self.producer.send_batch(batch, None).await.map_err(|e| {
            emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                error: e.to_string(),
            });
        })
    }

    /// Send a single event without batching.
    async fn send_single(
        &self,
        partition_id: Option<String>,
        body: Bytes,
        finalizers: EventFinalizers,
    ) -> Result<(), ()> {
        let event_data = azure_messaging_eventhubs::models::EventData::builder()
            .with_body(body.to_vec())
            .build();

        let options = partition_id.map(|pid| {
            azure_messaging_eventhubs::SendEventOptions {
                partition_id: Some(pid),
            }
        });

        match self.producer.send_event(event_data, options).await {
            Ok(_) => {
                finalizers.update_status(EventStatus::Delivered);
                Ok(())
            }
            Err(e) => {
                emit!(crate::internal_events::azure_event_hubs::sink::AzureEventHubsSendError {
                    error: e.to_string(),
                });
                finalizers.update_status(EventStatus::Errored);
                Ok(())
            }
        }
    }

    async fn run_inner(mut self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let mut input = input.fuse();

        if !self.batch_enabled {
            // Non-batch mode: send each event individually
            let mut sends_in_window: u64 = 0;
            let mut window_start = tokio::time::Instant::now();

            while let Some(event) = input.next().await {
                if let Some((partition_id, body, finalizers)) = self.encode_event(event) {
                    if window_start.elapsed() >= self.rate_limit_duration {
                        sends_in_window = 0;
                        window_start = tokio::time::Instant::now();
                    }
                    if sends_in_window < self.rate_limit_num {
                        self.send_single(partition_id, body, finalizers).await?;
                        sends_in_window += 1;
                    }
                }
            }
            return Ok(());
        }

        // Batch mode
        let mut buffer: Vec<(Option<String>, Bytes, EventFinalizers)> = Vec::new();
        let mut sends_in_window: u64 = 0;
        let mut window_start = tokio::time::Instant::now();

        loop {
            let timeout = sleep(self.batch_timeout);
            tokio::pin!(timeout);

            tokio::select! {
                event = input.next() => {
                    match event {
                        Some(event) => {
                            if let Some(encoded) = self.encode_event(event) {
                                buffer.push(encoded);
                            }

                            if buffer.len() >= self.batch_max_events {
                                // Rate limit check
                                if window_start.elapsed() >= self.rate_limit_duration {
                                    sends_in_window = 0;
                                    window_start = tokio::time::Instant::now();
                                }
                                if sends_in_window < self.rate_limit_num {
                                    self.flush_batches(&mut buffer).await?;
                                    sends_in_window += 1;
                                }
                            }
                        }
                        None => {
                            // Stream ended, flush remaining
                            self.flush_batches(&mut buffer).await?;
                            return Ok(());
                        }
                    }
                }
                _ = &mut timeout, if !buffer.is_empty() => {
                    // Rate limit check
                    if window_start.elapsed() >= self.rate_limit_duration {
                        sends_in_window = 0;
                        window_start = tokio::time::Instant::now();
                    }
                    if sends_in_window < self.rate_limit_num {
                        self.flush_batches(&mut buffer).await?;
                        sends_in_window += 1;
                    }
                }
            }
        }
    }
}

#[async_trait]
impl StreamSink<Event> for AzureEventHubsSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
