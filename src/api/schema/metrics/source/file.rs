use crate::{
    api::schema::{
        filter::{filter_items, CustomFilter, StringFilter},
        metrics::{self, MetricsFilter},
        relay,
    },
    event::Metric,
    filter_check,
};
use async_graphql::{InputObject, Object};
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct FileSourceMetricFile<'a> {
    name: String,
    metrics: Vec<&'a Metric>,
}

impl<'a> FileSourceMetricFile<'a> {
    /// Returns a new FileSourceMetricFile from a (name, Vec<&Metric>) tuple
    fn from_tuple((name, metrics): (String, Vec<&'a Metric>)) -> Self {
        Self { name, metrics }
    }

    pub fn get_name(&self) -> &str {
        self.name.as_str()
    }
}

#[Object]
impl<'a> FileSourceMetricFile<'a> {
    /// File name
    async fn name(&self) -> &str {
        &*self.name
    }

    /// Metric indicating events processed for the current file
    async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.metrics.processed_events_total()
    }

    /// Metric indicating bytes processed for the current file
    async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        self.metrics.processed_bytes_total()
    }
}

#[derive(Debug, Clone)]
pub struct FileSourceMetrics(Vec<Metric>);

impl FileSourceMetrics {
    pub fn new(metrics: Vec<Metric>) -> Self {
        Self(metrics)
    }

    pub fn get_files(&self) -> Vec<FileSourceMetricFile<'_>> {
        self.0
            .iter()
            .filter_map(|m| match m.tag_value("file") {
                Some(file) => Some((file, m)),
                _ => None,
            })
            .fold(BTreeMap::new(), |mut map, (file, m)| {
                map.entry(file).or_insert_with(Vec::new).push(m);
                map
            })
            .into_iter()
            .map(FileSourceMetricFile::from_tuple)
            .collect()
    }
}

#[Object]
impl FileSourceMetrics {
    /// File metrics
    pub async fn files(
        &self,
        after: Option<String>,
        before: Option<String>,
        first: Option<i32>,
        last: Option<i32>,
        filter: Option<FileSourceMetricsFilesFilter>,
    ) -> relay::ConnectionResult<FileSourceMetricFile<'_>> {
        let filter = filter.unwrap_or_else(FileSourceMetricsFilesFilter::default);
        let files = filter_items(self.get_files().into_iter(), &filter);

        relay::query(
            files.into_iter(),
            relay::Params::new(after, before, first, last),
            10,
        )
        .await
    }

    /// Events processed for the current file source
    pub async fn processed_events_total(&self) -> Option<metrics::ProcessedEventsTotal> {
        self.0.processed_events_total()
    }

    /// Bytes processed for the current file source
    pub async fn processed_bytes_total(&self) -> Option<metrics::ProcessedBytesTotal> {
        self.0.processed_bytes_total()
    }
}

#[derive(Default, InputObject)]
pub struct FileSourceMetricsFilesFilter {
    name: Option<Vec<StringFilter>>,
    or: Option<Vec<Self>>,
}

impl CustomFilter<FileSourceMetricFile<'_>> for FileSourceMetricsFilesFilter {
    fn matches(&self, file: &FileSourceMetricFile<'_>) -> bool {
        filter_check!(self
            .name
            .as_ref()
            .map(|f| f.iter().all(|f| f.filter_value(file.get_name()))));
        true
    }

    fn or(&self) -> Option<&Vec<Self>> {
        self.or.as_ref()
    }
}
