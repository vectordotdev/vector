use vector_core::event::Metric;

use crate::sinks::{prelude::*, prometheus::PrometheusRemoteWriteAuth};

use super::{
    request_builder::{RemoteWriteEncoder, RemoteWriteRequestBuilder},
    PrometheusMetricNormalize,
};

pub(super) struct RemoteWriteMetric {
    pub(super) metric: Metric,
    tenant_id: String,
}

impl GetEventCountTags for RemoteWriteMetric {
    fn get_tags(&self) -> TaggedEventsSent {
        self.metric.get_tags()
    }
}

impl EstimatedJsonEncodedSizeOf for RemoteWriteMetric {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.metric.estimated_json_encoded_size_of()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PrometheusRemoteWriteDefaultBatchSettings;

pub(super) struct PrometheusTenantIdPartitioner;

impl Partitioner for PrometheusTenantIdPartitioner {
    type Item = RemoteWriteMetric;
    type Key = String;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        // TODO do this better.
        item.tenant_id.clone()
    }
}

impl SinkBatchSettings for PrometheusRemoteWriteDefaultBatchSettings {
    const MAX_EVENTS: Option<usize> = Some(1_000);
    const MAX_BYTES: Option<usize> = None;
    const TIMEOUT_SECS: f64 = 1.0;
}

pub(super) struct RemoteWriteSink {
    tenant_id: Option<Template>,
    batch_settings: BatcherSettings,
    compression: super::Compression,
    http_auth: Option<PrometheusRemoteWriteAuth>,
    endpoint: String,
}

impl RemoteWriteSink {
    fn make_remote_write_event(&self, metric: Metric) -> Option<RemoteWriteMetric> {
        let tenant_id = self.tenant_id.as_ref().and_then(|template| {
            template
                .render_string(&metric)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("tenant_id"),
                        drop_event: true,
                    })
                })
                .ok()
        })?;

        Some(RemoteWriteMetric { metric, tenant_id })
    }

    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let request_builder = RemoteWriteRequestBuilder {
            endpoint: self.endpoint.clone(),
            compression: self.compression,
            encoder: RemoteWriteEncoder,
            http_auth: self.http_auth,
        };

        let service = RemoteWriteService {
            default_namespace: self.default_namespace.clone(),
            client,
            buckets,
            quantiles,
            http_request_builder,
            compression: self.compression,
        };
        let service = ServiceBuilder::new().service(service);

        input
            .filter_map(|event| future::ready(event.try_into_metric()))
            .normalized_with_default::<PrometheusMetricNormalize>()
            .filter_map(|event| self.make_remote_write_event(event))
            .batched_partitioned(PrometheusTenantIdPartitioner, self.batch_settings)
            .request_builder(request_builder)
            .into_driver(service)
            .protocol("http")
            .run()
            .await

        /*
            let buffer = PartitionBuffer::new(MetricsBuffer::new(batch.size));
            let mut normalizer = MetricNormalizer::<PrometheusMetricNormalize>::default();

            request_settings
                .partition_sink(HttpRetryLogic, service, buffer, batch.timeout)
                .with_flat_map(move |event: Event| {
                    let byte_size = event.size_of();
                    let json_size = event.estimated_json_encoded_size_of();

                    stream::iter(normalizer.normalize(event.into_metric()).map(|event| {
                        let tenant_id = tenant_id.as_ref().and_then(|template| {
                            template
                                .render_string(&event)
                                .map_err(|error| {
                                    emit!(TemplateRenderingError {
                                        error,
                                        field: Some("tenant_id"),
                                        drop_event: true,
                                    })
                                })
                                .ok()
                        });
                        let key = PartitionKey { tenant_id };
                        Ok(EncodedEvent::new(
                            PartitionInnerBuffer::new(event, key),
                            byte_size,
                            json_size,
                        ))
                    }))
                })
                .sink_map_err(
                    |error| error!(message = "Prometheus remote_write sink error.", %error),
                )
        ;
        */
    }
}

#[async_trait]
impl StreamSink<Event> for RemoteWriteSink {
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}
