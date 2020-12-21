use crate::{
    api::schema::metrics::{self, MetricsFilter},
    event::Metric,
};
use async_graphql::Object;
use itertools::Itertools;

#[derive(Debug, Clone)]
pub struct FileSourceMetrics(Vec<Metric>);

impl FileSourceMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }
}

pub struct FileSourceMetricFile<'a> {
    name: String,
    metrics: Vec<&'a Metric>,
}

impl<'a> FileSourceMetricFile<'a> {
    /// Returns a new FileSourceMetricFile from a (name, Vec<&Metric>) tuple
    fn from_tuple((name, metrics): (String, Vec<&'a Metric>)) -> Self {
        Self { name, metrics }
    }
}

#[Object]
impl FileSourceMetricFile<'_> {
    /// File name
    async fn name(&self) -> &str {
        &*self.name
    }

    /// Total events processed for the file
    async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.metrics.processed_events_total()
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

    /// File metrics
    pub async fn files(&self) -> Vec<FileSourceMetricFile<'_>> {
        self.0
            .iter()
            .filter_map(|m| match m.tag_value("file") {
                Some(file) => Some((file, m)),
                _ => None,
            })
            .into_group_map()
            .into_iter()
            .map(FileSourceMetricFile::from_tuple)
            .collect()
    }
}
