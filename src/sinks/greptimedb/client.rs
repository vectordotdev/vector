use std::sync::Arc;
use std::task::Poll;

use chrono::Utc;
use futures::stream;
use futures::SinkExt;
use futures_util::future::BoxFuture;
use greptimedb_client::api::v1::auth_header::AuthScheme;
use greptimedb_client::api::v1::column::*;
use greptimedb_client::api::v1::*;
use greptimedb_client::{
    Client, Database, Error as GreptimeError, DEFAULT_CATALOG_NAME, DEFAULT_SCHEMA_NAME,
};
use tower::Service;
use vector_core::event::metric::{Bucket, MetricSketch, Quantile, Sample};
use vector_core::event::{Event, Metric, MetricValue};
use vector_core::metrics::AgentDDSketch;
use vector_core::ByteSizeOf;

use super::GreptimeDBConfig;
use crate::sinks::util::buffer::metrics::MetricNormalize;
use crate::sinks::util::buffer::metrics::MetricNormalizer;
use crate::sinks::util::buffer::metrics::MetricSet;
use crate::sinks::util::buffer::metrics::MetricsBuffer;
use crate::sinks::util::retries::RetryLogic;
use crate::sinks::util::sink::Response;
use crate::sinks::util::statistic::DistributionStatistic;
use crate::sinks::util::{EncodedEvent, TowerRequestConfig};
use crate::sinks::VectorSink;

#[derive(Debug)]
pub struct GreptimeBatchOutput(Vec<u32>);

impl Response for GreptimeBatchOutput {}

#[derive(Clone)]
struct GreptimeDBRetryLogic;

impl RetryLogic for GreptimeDBRetryLogic {
    type Error = GreptimeError;
    type Response = GreptimeBatchOutput;

    fn is_retriable_error(&self, _error: &Self::Error) -> bool {
        true
    }
}

#[derive(Clone, Debug, Default)]
struct GreptimeDBMetricNormalize;

impl MetricNormalize for GreptimeDBMetricNormalize {
    fn normalize(&mut self, state: &mut MetricSet, metric: Metric) -> Option<Metric> {
        match (metric.kind(), &metric.value()) {
            (_, MetricValue::Counter { .. }) => state.make_absolute(metric),
            (_, MetricValue::Gauge { .. }) => state.make_absolute(metric),
            // All others are left as-is
            _ => Some(metric),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GreptimeDBService {
    /// the client that connects to greptimedb
    client: Arc<Database>,
}

impl GreptimeDBService {
    pub fn new_sink(config: &GreptimeDBConfig) -> crate::Result<VectorSink> {
        let grpc_client = Client::with_urls(vec![&config.grpc_endpoint]);
        let mut client = Database::new(
            config.catalog.as_deref().unwrap_or(DEFAULT_CATALOG_NAME),
            config.schema.as_deref().unwrap_or(DEFAULT_SCHEMA_NAME),
            grpc_client,
        );

        if let (Some(username), Some(password)) = (&config.username, &config.password) {
            client.set_auth(AuthScheme::Basic(Basic {
                username: username.to_owned(),
                password: password.clone().into(),
            }))
        }

        let batch = config.batch.into_batch_settings()?;
        let request = config.request.unwrap_with(&TowerRequestConfig {
            retry_attempts: Some(1),
            ..Default::default()
        });

        let greptime_service = GreptimeDBService {
            client: Arc::new(client),
        };

        let mut normalizer = MetricNormalizer::<GreptimeDBMetricNormalize>::default();

        let sink = request
            .batch_sink(
                GreptimeDBRetryLogic,
                greptime_service,
                MetricsBuffer::new(batch.size),
                batch.timeout,
            )
            .with_flat_map(move |event: Event| {
                stream::iter({
                    let byte_size = event.size_of();
                    normalizer
                        .normalize(event.into_metric())
                        .map(|metric| Ok(EncodedEvent::new(metric, byte_size)))
                })
            })
            .sink_map_err(|e| error!(message = "Fatal greptimedb sink error.", %e));

        Ok(VectorSink::from_event_sink(sink))
    }
}

impl Service<Vec<Metric>> for GreptimeDBService {
    type Response = GreptimeBatchOutput;
    type Error = GreptimeError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut std::task::Context) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    // Convert vector metrics into GreptimeDB format and send them in batch
    fn call(&mut self, items: Vec<Metric>) -> Self::Future {
        // TODO(sunng87): group metrics by name and send metrics with same name
        // in batch
        let requests = items.into_iter().map(metric_to_insert_request);
        let client = self.client.clone();

        Box::pin(async move {
            let mut outputs = Vec::with_capacity(requests.len());
            for request in requests {
                let result = client.insert(request).await?;
                outputs.push(result);
            }
            Ok(GreptimeBatchOutput(outputs))
        })
    }
}

fn f64_field(name: &str, value: f64) -> Column {
    Column {
        column_name: name.to_owned(),
        values: Some(column::Values {
            f64_values: vec![value],
            ..Default::default()
        }),
        semantic_type: SemanticType::Field as i32,
        datatype: ColumnDataType::Float64 as i32,
        ..Default::default()
    }
}

fn ts_column(name: &str, value: i64) -> Column {
    Column {
        column_name: name.to_owned(),
        values: Some(column::Values {
            ts_millisecond_values: vec![value],
            ..Default::default()
        }),
        semantic_type: SemanticType::Timestamp as i32,
        datatype: ColumnDataType::TimestampMillisecond as i32,
        ..Default::default()
    }
}

fn tag_column(name: &str, value: &str) -> Column {
    Column {
        column_name: name.to_owned(),
        values: Some(column::Values {
            string_values: vec![value.to_owned()],
            ..Default::default()
        }),
        semantic_type: SemanticType::Tag as i32,
        datatype: ColumnDataType::String as i32,
        ..Default::default()
    }
}

fn metric_to_insert_request(metric: Metric) -> InsertRequest {
    let ns = metric.namespace();
    let metric_name = metric.name();
    let table_name = if let Some(ns) = ns {
        format!("{ns}_{metric_name}")
    } else {
        metric_name.to_owned()
    };

    let mut columns = Vec::new();
    // timetamp
    let timestamp = metric
        .timestamp()
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    columns.push(ts_column("timestamp", timestamp));

    // tags
    if let Some(tags) = metric.tags() {
        for (key, value) in tags.iter_single() {
            columns.push(tag_column(key, value));
        }
    }

    // fields
    match metric.value() {
        MetricValue::Counter { value } => columns.push(f64_field("value", *value)),
        MetricValue::Gauge { value } => columns.push(f64_field("value", *value)),
        MetricValue::Set { values } => columns.push(f64_field("value", values.len() as f64)),
        MetricValue::Distribution { samples, .. } => {
            encode_distribution(samples, &mut columns);
        }

        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            encode_histogram(buckets.as_ref(), &mut columns);
            columns.push(f64_field("count", *count as f64));
            columns.push(f64_field("sum", *sum));
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            encode_quantiles(quantiles.as_ref(), &mut columns);
            columns.push(f64_field("count", *count as f64));
            columns.push(f64_field("sum", *sum));
        }
        MetricValue::Sketch { sketch } => {
            let MetricSketch::AgentDDSketch(sketch) = sketch;
            encode_sketch(&sketch, &mut columns);
        }
    }

    InsertRequest {
        table_name,
        columns,
        row_count: 1,
        ..Default::default()
    }
}

fn encode_distribution(samples: &[Sample], columns: &mut Vec<Column>) {
    if let Some(stats) = DistributionStatistic::from_samples(samples, &[0.75, 0.90, 0.95, 0.99]) {
        columns.push(f64_field("min", stats.min));
        columns.push(f64_field("max", stats.max));
        columns.push(f64_field("median", stats.median));
        columns.push(f64_field("avg", stats.avg));
        columns.push(f64_field("sum", stats.sum));
        columns.push(f64_field("count", stats.count as f64));

        for (quantile, value) in stats.quantiles {
            columns.push(f64_field(&format!("p{:2}", quantile * 100f64), value));
        }
    }
}

fn encode_histogram(buckets: &[Bucket], columns: &mut Vec<Column>) {
    for bucket in buckets {
        let column_name = format!("b{}", bucket.upper_limit);
        columns.push(f64_field(&column_name, bucket.count as f64));
    }
}

fn encode_quantiles(quantiles: &[Quantile], columns: &mut Vec<Column>) {
    for quantile in quantiles {
        let column_name = format!("p{:2}", quantile.quantile * 100f64);
        columns.push(f64_field(&column_name, quantile.value));
    }
}

fn encode_sketch(sketch: &AgentDDSketch, columns: &mut Vec<Column>) {
    columns.push(f64_field("count", sketch.count() as f64));
    if let Some(min) = sketch.min() {
        columns.push(f64_field("min", min));
    }

    if let Some(max) = sketch.max() {
        columns.push(f64_field("max", max));
    }

    if let Some(sum) = sketch.sum() {
        columns.push(f64_field("sum", sum));
    }

    if let Some(avg) = sketch.avg() {
        columns.push(f64_field("avg", avg));
    }

    if let Some(quantile) = sketch.quantile(0.5) {
        columns.push(f64_field("p50", quantile));
    }
    if let Some(quantile) = sketch.quantile(0.75) {
        columns.push(f64_field("p75", quantile));
    }
    if let Some(quantile) = sketch.quantile(0.90) {
        columns.push(f64_field("p90", quantile));
    }
    if let Some(quantile) = sketch.quantile(0.95) {
        columns.push(f64_field("p95", quantile));
    }
    if let Some(quantile) = sketch.quantile(0.99) {
        columns.push(f64_field("p99", quantile));
    }
}

#[cfg(test)]
mod tests {

    use similar_asserts::assert_eq;

    use super::*;
    use crate::event::metric::{MetricKind, StatisticKind};

    fn get_column(columns: &[Column], name: &str) -> f64 {
        let col = columns.iter().find(|c| c.column_name == name).unwrap();
        *(col.values.as_ref().unwrap().f64_values.get(0).unwrap())
    }

    #[test]
    fn test_metric_data_to_insert_request() {
        let metric = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        )
        .with_namespace(Some("ns"))
        .with_tags(Some([("host".to_owned(), "thinkneo".to_owned())].into()))
        .with_timestamp(Some(Utc::now()));

        let insert = metric_to_insert_request(metric);

        assert_eq!(insert.table_name, "ns_load1");
        assert_eq!(insert.row_count, 1);
        assert_eq!(insert.columns.len(), 3);

        let column_names = insert
            .columns
            .iter()
            .map(|c| c.column_name.as_ref())
            .collect::<Vec<&str>>();
        assert!(column_names.contains(&"timestamp"));
        assert!(column_names.contains(&"host"));
        assert!(column_names.contains(&"value"));

        assert_eq!(get_column(&insert.columns, "value"), 1.1);

        let metric2 = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        );
        let insert2 = metric_to_insert_request(metric2);
        assert_eq!(insert2.table_name, "load1");
    }

    #[test]
    fn test_counter() {
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.1 },
        );
        let insert = metric_to_insert_request(metric);
        assert_eq!(insert.columns.len(), 2);

        assert_eq!(get_column(&insert.columns, "value"), 1.1);
    }

    #[test]
    fn test_set() {
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Absolute,
            MetricValue::Set {
                values: ["foo".to_owned(), "bar".to_owned()].into_iter().collect(),
            },
        );
        let insert = metric_to_insert_request(metric);
        assert_eq!(insert.columns.len(), 2);

        assert_eq!(get_column(&insert.columns, "value"), 2.0);
    }

    #[test]
    fn test_distribution() {
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_core::samples![1.0 => 2, 2.0 => 4, 3.0 => 2],
                statistic: StatisticKind::Histogram,
            },
        );
        let insert = metric_to_insert_request(metric);
        assert_eq!(insert.columns.len(), 11);

        assert_eq!(get_column(&insert.columns, "max"), 3.0);
        assert_eq!(get_column(&insert.columns, "min"), 1.0);
        assert_eq!(get_column(&insert.columns, "median"), 2.0);
        assert_eq!(get_column(&insert.columns, "avg"), 2.0);
        assert_eq!(get_column(&insert.columns, "sum"), 16.0);
        assert_eq!(get_column(&insert.columns, "count"), 8.0);
        assert_eq!(get_column(&insert.columns, "p75"), 2.0);
        assert_eq!(get_column(&insert.columns, "p90"), 3.0);
        assert_eq!(get_column(&insert.columns, "p95"), 3.0);
        assert_eq!(get_column(&insert.columns, "p99"), 3.0);
    }

    #[test]
    fn test_histogram() {
        let metric = Metric::new(
            "cpu_seconds_totoal",
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets: vector_core::buckets![1.0 => 1, 2.0 => 2, 3.0 => 1],
                count: 4,
                sum: 8.0,
            },
        );
        let insert = metric_to_insert_request(metric);
        assert_eq!(insert.columns.len(), 5);

        assert_eq!(get_column(&insert.columns, "b1.0"), 1.0);
        assert_eq!(get_column(&insert.columns, "b2.0"), 2.0);
        assert_eq!(get_column(&insert.columns, "b3.0"), 1.0);
        assert_eq!(get_column(&insert.columns, "count"), 4.0);
        assert_eq!(get_column(&insert.columns, "sum"), 8.0);
    }

    #[test]
    fn test_summary() {
        let metric = Metric::new(
            "cpu_seconds_totoal",
            MetricKind::Incremental,
            MetricValue::AggregatedSummary {
                quantiles: vector_core::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0],
                count: 6,
                sum: 12.0,
            },
        );

        let insert = metric_to_insert_request(metric);
        assert_eq!(insert.columns.len(), 5);

        assert_eq!(get_column(&insert.columns, "p01"), 1.5);
        assert_eq!(get_column(&insert.columns, "p50"), 2.0);
        assert_eq!(get_column(&insert.columns, "p99"), 3.0);
        assert_eq!(get_column(&insert.columns, "count"), 6.0);
        assert_eq!(get_column(&insert.columns, "sum"), 12.0);
    }
}
