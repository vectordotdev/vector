#![deny(warnings)]

use vrl::compiler::Function;

mod aggregate_vector_metrics;
mod common;
mod find_vector_metrics;
mod get_vector_metric;
pub use common::MetricsStorage;

pub(crate) const VECTOR_METRICS_EXPLAINER: &str = "\
Internal Vector metrics functions work with a snapshot of the metrics. The interval at which \
the snapshot is updated is controlled through the \
`metrics_storage_refresh_period` (/docs/reference/configuration/global-options/#metrics_storage_refresh_period) \
global option. Higher values can reduce performance impact of that process, but may cause \
stale metrics data in the snapshot.";

pub fn all() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(get_vector_metric::GetVectorMetric) as _,
        Box::new(find_vector_metrics::FindVectorMetrics) as _,
        Box::new(aggregate_vector_metrics::AggregateVectorMetrics) as _,
    ]
}
