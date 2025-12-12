#![deny(warnings)]

use vrl::compiler::Function;

mod aggregate_vector_metrics;
mod common;
mod find_vector_metrics;
mod get_vector_metric;
pub use common::MetricsStorage;

pub fn all() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(get_vector_metric::GetVectorMetric) as _,
        Box::new(find_vector_metrics::FindVectorMetrics) as _,
        Box::new(aggregate_vector_metrics::AggregateVectorMetrics) as _,
    ]
}
