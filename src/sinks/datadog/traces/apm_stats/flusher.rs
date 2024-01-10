use std::{
    io::Write,
    sync::{Arc, Mutex},
};

use bytes::Bytes;
use snafu::ResultExt;
use tokio::sync::oneshot::{Receiver, Sender};
use vector_lib::{finalization::EventFinalizers, request_metadata::RequestMetadata};

use super::{
    aggregation::Aggregator, build_request, DDTracesMetadata, DatadogTracesEndpoint,
    DatadogTracesEndpointConfiguration, RequestBuilderError, StatsPayload,
    BUCKET_DURATION_NANOSECONDS,
};
use crate::{
    http::{BuildRequestSnafu, HttpClient},
    internal_events::DatadogTracesAPMStatsError,
    sinks::util::{Compression, Compressor},
};

/// Flushes cached APM stats buckets to Datadog on a 10 second interval.
/// When the sink signals this thread that it is shutting down, all remaining
/// buckets are flush before the thread exits.
///
/// # arguments
///
/// * `tripwire`                 - Receiver that the sink signals when shutting down.
/// * `client`                   - HttpClient to use in sending the stats payloads.
/// * `compression`              - Compression to use when creating the HTTP requests.
/// * `endpoint_configuration`   - Endpoint configuration to use when creating the HTTP requests.
/// * `aggregator`               - The Aggregator object containing cached stats buckets.
pub async fn flush_apm_stats_thread(
    mut tripwire: Receiver<Sender<()>>,
    client: HttpClient,
    compression: Compression,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    aggregator: Arc<Mutex<Aggregator>>,
) {
    let sender = ApmStatsSender {
        client,
        compression,
        endpoint_configuration,
        aggregator,
    };

    // flush on the same interval as the stats buckets
    let mut interval =
        tokio::time::interval(std::time::Duration::from_nanos(BUCKET_DURATION_NANOSECONDS));

    debug!("Starting APM stats flushing thread.");

    loop {
        tokio::select! {

        _ = interval.tick() => {
            // flush the oldest bucket from the cache to Datadog
            sender.flush_apm_stats(false).await;
        },
        signal = &mut tripwire =>  match signal {
            // sink has signaled us that the process is shutting down
            Ok(sink_shutdown_ack_sender) => {

                debug!("APM stats flushing thread received exit condition. Flushing remaining stats before exiting.");
                sender.flush_apm_stats(true).await;

                // signal the sink (who tripped the tripwire), that we are done flushing
                _ = sink_shutdown_ack_sender.send(());
                break;
            }
            Err(_) => {
                error!(
                    internal_log_rate_limit = true,
                    message = "Tokio Sender unexpectedly dropped."
                );
                break;
            },
        }
        }
    }
}

struct ApmStatsSender {
    client: HttpClient,
    compression: Compression,
    endpoint_configuration: DatadogTracesEndpointConfiguration,
    aggregator: Arc<Mutex<Aggregator>>,
}

impl ApmStatsSender {
    async fn flush_apm_stats(&self, force: bool) {
        // explicit scope to minimize duration that the Aggregator is locked.
        if let Some((payload, api_key)) = {
            let mut aggregator = self.aggregator.lock().unwrap();
            let client_stats_payloads = aggregator.flush(force);

            if client_stats_payloads.is_empty() {
                // no sense proceeding if no payloads to flush
                None
            } else {
                let payload = StatsPayload {
                    agent_hostname: aggregator.get_agent_hostname(),
                    agent_env: aggregator.get_agent_env(),
                    stats: client_stats_payloads,
                    agent_version: aggregator.get_agent_version(),
                    client_computed: false,
                };

                Some((payload, aggregator.get_api_key()))
            }
        } {
            if let Err(error) = self.compress_and_send(payload, api_key).await {
                emit!(DatadogTracesAPMStatsError { error });
            }
        }
    }

    async fn compress_and_send(
        &self,
        payload: StatsPayload,
        api_key: Arc<str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (metadata, compressed_payload) = self.build_apm_stats_request_data(api_key, payload)?;

        let request_metadata = RequestMetadata::default();
        let trace_api_request = build_request(
            (metadata, request_metadata),
            compressed_payload,
            self.compression,
            &self.endpoint_configuration,
        );

        let http_request = trace_api_request
            .into_http_request()
            .context(BuildRequestSnafu)?;

        self.client.send(http_request).await?;

        Ok(())
    }

    fn build_apm_stats_request_data(
        &self,
        api_key: Arc<str>,
        payload: StatsPayload,
    ) -> Result<(DDTracesMetadata, Bytes), RequestBuilderError> {
        let encoded_payload =
            rmp_serde::to_vec_named(&payload).map_err(|e| RequestBuilderError::FailedToBuild {
                message: "Encoding failed.",
                reason: e.to_string(),
                dropped_events: 0,
            })?;
        let uncompressed_size = encoded_payload.len();
        let metadata = DDTracesMetadata {
            api_key,
            endpoint: DatadogTracesEndpoint::APMStats,
            finalizers: EventFinalizers::default(),
            uncompressed_size,
            content_type: "application/msgpack".to_string(),
        };

        let mut compressor = Compressor::from(self.compression);
        match compressor.write_all(&encoded_payload) {
            Ok(()) => {
                let bytes = compressor.into_inner().freeze();

                Ok((metadata, bytes))
            }
            Err(e) => Err(RequestBuilderError::FailedToBuild {
                message: "Compression failed.",
                reason: e.to_string(),
                dropped_events: 0,
            }),
        }
    }
}
