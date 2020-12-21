mod file;
mod generic;

use crate::event::Metric;
use async_graphql::Union;

#[derive(Debug, Clone, Union)]
pub enum SourceMetrics {
    FileSourceMetrics(file::FileSourceMetrics),
}

pub trait IntoSourceMetrics {
    fn to_source_metrics(self, component_type: &str) -> Option<SourceMetrics>;
}

impl IntoSourceMetrics for Vec<Metric> {
    fn to_source_metrics(self, component_type: &str) -> Option<SourceMetrics> {
        match component_type {
            "file" => Some(SourceMetrics::FileSourceMetrics(
                file::FileSourceMetrics::new(self),
            )),
            _ => None,
        }
    }
}
