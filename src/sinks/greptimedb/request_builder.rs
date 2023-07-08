use chrono::Utc;
use greptimedb_client::api::v1::column::*;
use greptimedb_client::api::v1::*;
use vector_core::event::metric::{Bucket, MetricSketch, Quantile, Sample};
use vector_core::event::{Metric, MetricValue};
use vector_core::metrics::AgentDDSketch;

use crate::sinks::util::statistic::DistributionStatistic;

pub(super) const DISTRIBUTION_QUANTILES: [f64; 5] = [0.5, 0.75, 0.90, 0.95, 0.99];
pub(super) const DISTRIBUTION_STAT_FIELD_COUNT: usize = 5;
pub(super) const SUMMARY_STAT_FIELD_COUNT: usize = 2;

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

pub(super) fn metric_to_insert_request(metric: Metric) -> InsertRequest {
    let ns = metric.namespace();
    let metric_name = metric.name();
    let table_name = if let Some(ns) = ns {
        format!("{ns}_{metric_name}")
    } else {
        metric_name.to_owned()
    };

    let mut columns = Vec::new();
    // timestamp
    let timestamp = metric
        .timestamp()
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    columns.push(ts_column("ts", timestamp));

    // tags
    if let Some(tags) = metric.tags() {
        for (key, value) in tags.iter_single() {
            columns.push(tag_column(key, value));
        }
    }

    // fields
    match metric.value() {
        MetricValue::Counter { value } => columns.push(f64_field("val", *value)),
        MetricValue::Gauge { value } => columns.push(f64_field("val", *value)),
        MetricValue::Set { values } => columns.push(f64_field("val", values.len() as f64)),
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
            encode_sketch(sketch, &mut columns);
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
    if let Some(stats) = DistributionStatistic::from_samples(samples, &DISTRIBUTION_QUANTILES) {
        columns.push(f64_field("min", stats.min));
        columns.push(f64_field("max", stats.max));
        columns.push(f64_field("avg", stats.avg));
        columns.push(f64_field("sum", stats.sum));
        columns.push(f64_field("count", stats.count as f64));

        for (quantile, value) in stats.quantiles {
            columns.push(f64_field(&format!("p{:02}", quantile * 100f64), value));
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
        let column_name = format!("p{:02}", quantile.quantile * 100f64);
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

    for q in DISTRIBUTION_QUANTILES {
        if let Some(quantile) = sketch.quantile(q) {
            let column_name = format!("p{:02}", q * 100f64);
            columns.push(f64_field(&column_name, quantile));
        }
    }
}

#[cfg(test)]
mod tests {

    use similar_asserts::assert_eq;

    use super::*;
    use crate::event::metric::{MetricKind, StatisticKind};

    fn get_column(columns: &[Column], name: &str) -> f64 {
        let col = columns.iter().find(|c| c.column_name == name).unwrap();
        *(col.values.as_ref().unwrap().f64_values.first().unwrap())
    }

    #[test]
    fn test_metric_data_to_insert_request() {
        let metric = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        )
        .with_namespace(Some("ns"))
        .with_tags(Some([("host".to_owned(), "my_host".to_owned())].into()))
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
        assert!(column_names.contains(&"ts"));
        assert!(column_names.contains(&"host"));
        assert!(column_names.contains(&"val"));

        assert_eq!(get_column(&insert.columns, "val"), 1.1);

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

        assert_eq!(get_column(&insert.columns, "val"), 1.1);
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

        assert_eq!(get_column(&insert.columns, "val"), 2.0);
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
        assert_eq!(
            insert.columns.len(),
            1 + DISTRIBUTION_STAT_FIELD_COUNT + DISTRIBUTION_QUANTILES.len()
        );

        assert_eq!(get_column(&insert.columns, "max"), 3.0);
        assert_eq!(get_column(&insert.columns, "min"), 1.0);
        assert_eq!(get_column(&insert.columns, "avg"), 2.0);
        assert_eq!(get_column(&insert.columns, "sum"), 16.0);
        assert_eq!(get_column(&insert.columns, "count"), 8.0);
        assert_eq!(get_column(&insert.columns, "p50"), 2.0);
        assert_eq!(get_column(&insert.columns, "p75"), 2.0);
        assert_eq!(get_column(&insert.columns, "p90"), 3.0);
        assert_eq!(get_column(&insert.columns, "p95"), 3.0);
        assert_eq!(get_column(&insert.columns, "p99"), 3.0);
    }

    #[test]
    fn test_histogram() {
        let buckets = vector_core::buckets![1.0 => 1, 2.0 => 2, 3.0 => 1];
        let buckets_len = buckets.len();
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::AggregatedHistogram {
                buckets,
                count: 4,
                sum: 8.0,
            },
        );
        let insert = metric_to_insert_request(metric);
        assert_eq!(
            insert.columns.len(),
            1 + SUMMARY_STAT_FIELD_COUNT + buckets_len
        );

        assert_eq!(get_column(&insert.columns, "b1"), 1.0);
        assert_eq!(get_column(&insert.columns, "b2"), 2.0);
        assert_eq!(get_column(&insert.columns, "b3"), 1.0);
        assert_eq!(get_column(&insert.columns, "count"), 4.0);
        assert_eq!(get_column(&insert.columns, "sum"), 8.0);
    }

    #[test]
    fn test_summary() {
        let quantiles = vector_core::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0];
        let quantiles_len = quantiles.len();
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::AggregatedSummary {
                quantiles,
                count: 6,
                sum: 12.0,
            },
        );

        let insert = metric_to_insert_request(metric);
        assert_eq!(
            insert.columns.len(),
            1 + SUMMARY_STAT_FIELD_COUNT + quantiles_len
        );

        assert_eq!(get_column(&insert.columns, "p01"), 1.5);
        assert_eq!(get_column(&insert.columns, "p50"), 2.0);
        assert_eq!(get_column(&insert.columns, "p99"), 3.0);
        assert_eq!(get_column(&insert.columns, "count"), 6.0);
        assert_eq!(get_column(&insert.columns, "sum"), 12.0);
    }

    #[test]
    fn test_sketch() {
        let mut sketch = AgentDDSketch::with_agent_defaults();
        let samples = 10;
        for i in 0..samples {
            sketch.insert(i as f64);
        }

        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::Sketch {
                sketch: MetricSketch::AgentDDSketch(sketch),
            },
        );

        let insert = metric_to_insert_request(metric);
        assert_eq!(
            insert.columns.len(),
            1 + DISTRIBUTION_QUANTILES.len() + DISTRIBUTION_STAT_FIELD_COUNT
        );

        assert!(get_column(&insert.columns, "p50") <= 4.0);
        assert!(get_column(&insert.columns, "p95") > 8.0);
        assert!(get_column(&insert.columns, "p95") <= 9.0);
        assert!(get_column(&insert.columns, "p99") > 8.0);
        assert!(get_column(&insert.columns, "p99") <= 9.0);
        assert_eq!(get_column(&insert.columns, "count"), samples as f64);
        assert_eq!(get_column(&insert.columns, "sum"), 45.0);
        assert_eq!(get_column(&insert.columns, "max"), 9.0);
        assert_eq!(get_column(&insert.columns, "min"), 0.0);
    }
}
