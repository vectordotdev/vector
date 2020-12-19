use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};
use async_graphql::Object;

#[derive(Debug, Clone)]
pub struct FileSourceMetrics(Vec<Metric>);

impl FileSourceMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

pub struct FileSourceMetricFile<'a>(&'a Metric);

impl<'a> FileSourceMetricFile<'a> {
    fn new(metric: &'a Metric) -> Self {
        Self(metric)
    }
}

#[Object]
impl FileSourceMetricFile<'_> {
    /// File name
    async fn name(&self) -> Option<String> {
        self.0.tag_value("file")
    }
}

#[Object]
impl FileSourceMetrics {
    /// Metric indicating events processed for the current file source
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.0.processed_events_total()
    }

    /// Metric indicating bytes processed for the current file source
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        self.0.processed_bytes_total()
    }

    pub async fn files(&self) -> Vec<FileSourceMetricFile<'_>> {
        self.0.iter().map(FileSourceMetricFile::new).collect()
    }
}
