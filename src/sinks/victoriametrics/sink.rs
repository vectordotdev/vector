use std::{fmt, time::Duration};

use vector_lib::{
    byte_size_of::ByteSizeOf, event::Metric, stream::batcher::limiter::ItemBatchSize,
};

use super::{
    PartitionKey, TOKEN_SECRET_KEY, Tenant,
    config::TenantMode,
    encoder::estimated_encoded_len,
    histogram::VmHistogram,
    normalize::{NormalizedMetric, VmNormalizer},
    request_builder::{VmRequest, VmRequestBuilder},
};
use crate::{internal_events::VictoriaMetricsInvalidTenantError, sinks::prelude::*};

/// A normalized metric with its routing information.
pub(super) struct VmMetric {
    pub(super) metric: Metric,
    /// The cumulative histogram of a distribution metric.
    pub(super) histogram: Option<VmHistogram>,
    /// The tenant written as `vm_account_id`/`vm_project_id` labels (`labels` mode).
    pub(super) labels_tenant: Option<Tenant>,
    key: PartitionKey,
}

impl VmMetric {
    pub(super) fn new(
        metric: Metric,
        histogram: Option<VmHistogram>,
        labels_tenant: Option<Tenant>,
        path_tenant: Option<Tenant>,
    ) -> Self {
        let token = metric.metadata().secrets().get(TOKEN_SECRET_KEY).cloned();
        Self {
            metric,
            histogram,
            labels_tenant,
            key: PartitionKey {
                tenant: path_tenant,
                token,
            },
        }
    }
}

impl Finalizable for VmMetric {
    fn take_finalizers(&mut self) -> EventFinalizers {
        self.metric.take_finalizers()
    }
}

impl GetEventCountTags for VmMetric {
    fn get_tags(&self) -> TaggedEventsSent {
        self.metric.get_tags()
    }
}

impl EstimatedJsonEncodedSizeOf for VmMetric {
    fn estimated_json_encoded_size_of(&self) -> JsonSize {
        self.metric.estimated_json_encoded_size_of()
    }
}

impl ByteSizeOf for VmMetric {
    fn allocated_bytes(&self) -> usize {
        self.metric.allocated_bytes() + self.histogram.allocated_bytes()
    }
}

/// Sizes batch items by their estimated encoded size, so that `batch.max_bytes` bounds the
/// uncompressed request size like `-remoteWrite.maxBlockSize` does in `vmagent`. A single
/// distribution can expand to hundreds of series, so the in-memory size is not a good bound.
struct VmItemSize;

impl ItemBatchSize<VmMetric> for VmItemSize {
    fn size(&self, item: &VmMetric) -> usize {
        estimated_encoded_len(item)
    }
}

struct VmPartitioner;

impl Partitioner for VmPartitioner {
    type Item = VmMetric;
    type Key = PartitionKey;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        item.key.clone()
    }
}

pub(super) struct VmSink<S> {
    pub(super) service: S,
    pub(super) batch_settings: BatcherSettings,
    pub(super) request_builder: VmRequestBuilder,
    pub(super) tenant_mode: TenantMode,
    pub(super) tenant_id: Option<ConfinedTemplate>,
    pub(super) expire_metrics: Option<Duration>,
}

impl<S> VmSink<S>
where
    S: Service<VmRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run_inner(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        let batch_settings = self.batch_settings;
        let tenant_mode = self.tenant_mode;
        let tenant_id = self.tenant_id;
        let mut normalizer = VmNormalizer::new(self.expire_metrics);

        input
            .filter_map(|event| future::ready(event.try_into_metric()))
            .filter_map(move |metric| future::ready(normalizer.normalize(metric)))
            .filter_map(move |normalized| {
                future::ready(route_metric(normalized, tenant_mode, tenant_id.as_ref()))
            })
            .batched_partitioned(VmPartitioner, batch_settings.timeout, |_| {
                batch_settings.as_item_size_config(VmItemSize)
            })
            .request_builder(
                default_request_builder_concurrency_limit(),
                self.request_builder,
            )
            .filter_map(|request| async move {
                match request {
                    Err(error) => {
                        emit!(SinkRequestBuildError { error });
                        None
                    }
                    Ok(request) => Some(request),
                }
            })
            .into_driver(self.service)
            .run()
            .await
    }
}

#[async_trait]
impl<S> StreamSink<Event> for VmSink<S>
where
    S: Service<VmRequest> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: DriverResponse + Send + 'static,
    S::Error: fmt::Debug + Into<crate::Error> + Send,
{
    async fn run(self: Box<Self>, input: BoxStream<'_, Event>) -> Result<(), ()> {
        self.run_inner(input).await
    }
}

/// Resolves the tenant of a metric. Metrics whose tenant cannot be resolved are rejected.
fn route_metric(
    normalized: NormalizedMetric,
    tenant_mode: TenantMode,
    tenant_id: Option<&ConfinedTemplate>,
) -> Option<VmMetric> {
    let NormalizedMetric {
        mut metric,
        histogram,
    } = normalized;

    let tenant = match tenant_id {
        None => None,
        Some(template) => {
            let rendered = match template.render_string(&metric) {
                Ok(rendered) => rendered,
                Err(error) => {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("tenant.id"),
                        drop_event: true,
                    });
                    metric
                        .take_finalizers()
                        .update_status(EventStatus::Rejected);
                    return None;
                }
            };
            match Tenant::parse(&rendered) {
                Some(tenant) => Some(tenant),
                None => {
                    emit!(VictoriaMetricsInvalidTenantError { tenant: &rendered });
                    metric
                        .take_finalizers()
                        .update_status(EventStatus::Rejected);
                    return None;
                }
            }
        }
    };

    Some(match tenant_mode {
        TenantMode::None => VmMetric::new(metric, histogram, None, None),
        TenantMode::Path => VmMetric::new(metric, histogram, None, tenant),
        TenantMode::Labels => VmMetric::new(metric, histogram, tenant, None),
    })
}
