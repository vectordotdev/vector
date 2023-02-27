use greptime_client::api::v1::*;
use greptime_client::{Client, Database, Error as GreptimeError, Output};

use super::GreptimeDBConfig;
use crate::sinks::VectorSink;

#[derive(Clone)]
struct GreptimeDBRetryLogic;

impl RetryLogic for GreptimeDBRetryLogic {
    type Error = GreptimeError;
    type Response = Output;

    fn is_retriable_error(&self, error: &Self::Error) -> bool {
        // TODO(sunng87): implement this
        false
    }
}

#[derive(Debug)]
pub struct GreptimeDBService {
    /// the client that connects to greptimedb
    client: Database,
}

impl GreptimeDBService {
    pub fn new_sink(config: &GreptimeDBConfig) -> crate::Result<VectorSink> {
        let grpc_client = Client::with_urls(vec![config.endpoint]);
        let client = Database::new(config.catalog, config.schema, grpc_client);

        let batch = config.batch.into_batch_settings()?;
        let request = config.request.unwrap_with(&TowerRequestConfig {
            retry_attempts: Some(5),
            ..Default::default()
        });

        let greptime_service = GreptimeDBService { client };
        let sink = request
            .batch_sink(
                GreptimeDBRetryLogic,
                greptime_service,
                MetricsBuffer::new(batch.size),
                batch.timeout,
            )
            .with_flat_map(move |event: Event| {
                // TODO(sunng87):
                stream::iter({
                    let byte_size = event.size_of();
                    normalizer
                        .normalize(event.into_metric())
                        .map(|metric| Ok(EncodedEvent::new(metric, byte_size)))
                })
            })
            .sink_map_err(|e| error!(message = "Fatal greptimedb sink error.", %e));

        Ok(VectorSink::From_event_sink(sink))
    }
}

impl Service<Vec<Metric>> for GreptimeDBService {
    type Response = Output;
    type Error = GreptimeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    // Emission of Error internal event is handled upstream by the caller
    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        // TODO(sunng87):
        todo!()
    }
}
