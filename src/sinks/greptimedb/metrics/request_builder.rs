use crate::sinks::util::statistic::DistributionStatistic;
use chrono::Utc;
use greptimedb_ingester::{api::v1::*, helpers::values::*};
use vector_lib::{
    event::{
        metric::{Bucket, MetricSketch, Quantile, Sample},
        Metric, MetricValue,
    },
    metrics::AgentDDSketch,
};

pub(super) struct RequestBuilderOptions {
    pub(super) use_new_naming: bool,
}

pub(super) const DISTRIBUTION_QUANTILES: [f64; 5] = [0.5, 0.75, 0.90, 0.95, 0.99];
pub(super) const DISTRIBUTION_STAT_FIELD_COUNT: usize = 5;
pub(super) const SUMMARY_STAT_FIELD_COUNT: usize = 2;
pub(super) const LEGACY_TIME_INDEX_COLUMN_NAME: &str = "ts";
pub(super) const TIME_INDEX_COLUMN_NAME: &str = "greptime_timestamp";
pub(super) const LEGACY_VALUE_COLUMN_NAME: &str = "val";
pub(super) const VALUE_COLUMN_NAME: &str = "greptime_value";

fn encode_f64_value(
    name: &str,
    value: f64,
    schema: &mut Vec<ColumnSchema>,
    columns: &mut Vec<Value>,
) {
    schema.push(f64_column(name));
    columns.push(f64_value(value));
}

pub fn metric_to_insert_request(
    metric: Metric,
    options: &RequestBuilderOptions,
) -> RowInsertRequest {
    let ns = metric.namespace();
    let metric_name = metric.name();
    let table_name = if let Some(ns) = ns {
        format!("{ns}_{metric_name}")
    } else {
        metric_name.to_owned()
    };
    let mut schema = Vec::new();
    let mut columns = Vec::new();

    // timestamp
    let timestamp = metric
        .timestamp()
        .map(|t| t.timestamp_millis())
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    schema.push(ts_column(if options.use_new_naming {
        TIME_INDEX_COLUMN_NAME
    } else {
        LEGACY_TIME_INDEX_COLUMN_NAME
    }));
    columns.push(timestamp_millisecond_value(timestamp));

    // tags
    if let Some(tags) = metric.tags() {
        for (key, value) in tags.iter_single() {
            schema.push(tag_column(key));
            columns.push(string_value(value.to_owned()));
        }
    }

    // fields
    match metric.value() {
        MetricValue::Counter { value } => {
            encode_f64_value(
                if options.use_new_naming {
                    VALUE_COLUMN_NAME
                } else {
                    LEGACY_VALUE_COLUMN_NAME
                },
                *value,
                &mut schema,
                &mut columns,
            );
        }
        MetricValue::Gauge { value } => {
            encode_f64_value(
                if options.use_new_naming {
                    VALUE_COLUMN_NAME
                } else {
                    LEGACY_VALUE_COLUMN_NAME
                },
                *value,
                &mut schema,
                &mut columns,
            );
        }
        MetricValue::Set { values } => {
            encode_f64_value(
                if options.use_new_naming {
                    VALUE_COLUMN_NAME
                } else {
                    LEGACY_VALUE_COLUMN_NAME
                },
                values.len() as f64,
                &mut schema,
                &mut columns,
            );
        }
        MetricValue::Distribution { samples, .. } => {
            encode_distribution(samples, &mut schema, &mut columns);
        }

        MetricValue::AggregatedHistogram {
            buckets,
            count,
            sum,
        } => {
            encode_histogram(buckets.as_ref(), &mut schema, &mut columns);
            encode_f64_value("count", *count as f64, &mut schema, &mut columns);
            encode_f64_value("sum", *sum, &mut schema, &mut columns);
        }
        MetricValue::AggregatedSummary {
            quantiles,
            count,
            sum,
        } => {
            encode_quantiles(quantiles.as_ref(), &mut schema, &mut columns);
            encode_f64_value("count", *count as f64, &mut schema, &mut columns);
            encode_f64_value("sum", *sum, &mut schema, &mut columns);
        }
        MetricValue::Sketch { sketch } => {
            let MetricSketch::AgentDDSketch(sketch) = sketch;
            encode_sketch(sketch, &mut schema, &mut columns);
        }
    }

    RowInsertRequest {
        table_name,
        rows: Some(Rows {
            schema,
            rows: vec![Row { values: columns }],
        }),
    }
}

fn encode_distribution(
    samples: &[Sample],
    schema: &mut Vec<ColumnSchema>,
    columns: &mut Vec<Value>,
) {
    if let Some(stats) = DistributionStatistic::from_samples(samples, &DISTRIBUTION_QUANTILES) {
        encode_f64_value("min", stats.min, schema, columns);
        encode_f64_value("max", stats.max, schema, columns);
        encode_f64_value("avg", stats.avg, schema, columns);
        encode_f64_value("sum", stats.sum, schema, columns);
        encode_f64_value("count", stats.count as f64, schema, columns);

        for (quantile, value) in stats.quantiles {
            encode_f64_value(
                &format!("p{:02}", quantile * 100f64),
                value,
                schema,
                columns,
            );
        }
    }
}

fn encode_histogram(buckets: &[Bucket], schema: &mut Vec<ColumnSchema>, columns: &mut Vec<Value>) {
    for bucket in buckets {
        let column_name = format!("b{}", bucket.upper_limit);
        encode_f64_value(&column_name, bucket.count as f64, schema, columns);
    }
}

fn encode_quantiles(
    quantiles: &[Quantile],
    schema: &mut Vec<ColumnSchema>,
    columns: &mut Vec<Value>,
) {
    for quantile in quantiles {
        let column_name = format!("p{:02}", quantile.quantile * 100f64);
        encode_f64_value(&column_name, quantile.value, schema, columns);
    }
}

fn encode_sketch(sketch: &AgentDDSketch, schema: &mut Vec<ColumnSchema>, columns: &mut Vec<Value>) {
    encode_f64_value("count", sketch.count() as f64, schema, columns);
    if let Some(min) = sketch.min() {
        encode_f64_value("min", min, schema, columns);
    }

    if let Some(max) = sketch.max() {
        encode_f64_value("max", max, schema, columns);
    }

    if let Some(sum) = sketch.sum() {
        encode_f64_value("sum", sum, schema, columns);
    }

    if let Some(avg) = sketch.avg() {
        encode_f64_value("avg", avg, schema, columns);
    }

    for q in DISTRIBUTION_QUANTILES {
        if let Some(quantile) = sketch.quantile(q) {
            let column_name = format!("p{:02}", q * 100f64);
            encode_f64_value(&column_name, quantile, schema, columns);
        }
    }
}

fn f64_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Field as i32,
        datatype: ColumnDataType::Float64 as i32,
        ..Default::default()
    }
}

fn ts_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Timestamp as i32,
        datatype: ColumnDataType::TimestampMillisecond as i32,
        ..Default::default()
    }
}

fn tag_column(name: &str) -> ColumnSchema {
    ColumnSchema {
        column_name: name.to_owned(),
        semantic_type: SemanticType::Tag as i32,
        datatype: ColumnDataType::String as i32,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {

    use similar_asserts::assert_eq;

    use super::*;
    use crate::event::metric::{MetricKind, StatisticKind};

    fn get_column(rows: &Rows, name: &str) -> f64 {
        let (col_index, _) = rows
            .schema
            .iter()
            .enumerate()
            .find(|(_, c)| c.column_name == name)
            .unwrap();
        let value_data = rows.rows[0].values[col_index]
            .value_data
            .as_ref()
            .expect("null value");
        match value_data {
            value::ValueData::F64Value(v) => *v,
            _ => {
                unreachable!()
            }
        }
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

        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);

        assert_eq!(insert.table_name, "ns_load1");
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(rows.rows[0].values.len(), 3);

        let column_names = rows
            .schema
            .iter()
            .map(|c| c.column_name.as_ref())
            .collect::<Vec<&str>>();
        assert!(column_names.contains(&LEGACY_TIME_INDEX_COLUMN_NAME));
        assert!(column_names.contains(&"host"));
        assert!(column_names.contains(&LEGACY_VALUE_COLUMN_NAME));

        assert_eq!(get_column(&rows, LEGACY_VALUE_COLUMN_NAME), 1.1);

        let metric2 = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        );
        let insert2 = metric_to_insert_request(metric2, &options);
        assert_eq!(insert2.table_name, "load1");
    }

    #[test]
    fn test_metric_data_to_insert_request_new_naming() {
        let metric = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        )
        .with_namespace(Some("ns"))
        .with_tags(Some([("host".to_owned(), "my_host".to_owned())].into()))
        .with_timestamp(Some(Utc::now()));

        let options = RequestBuilderOptions {
            use_new_naming: true,
        };

        let insert = metric_to_insert_request(metric, &options);

        assert_eq!(insert.table_name, "ns_load1");
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(rows.rows.len(), 1);
        assert_eq!(rows.rows[0].values.len(), 3);

        let column_names = rows
            .schema
            .iter()
            .map(|c| c.column_name.as_ref())
            .collect::<Vec<&str>>();
        assert!(column_names.contains(&TIME_INDEX_COLUMN_NAME));
        assert!(column_names.contains(&"host"));
        assert!(column_names.contains(&VALUE_COLUMN_NAME));

        assert_eq!(get_column(&rows, VALUE_COLUMN_NAME), 1.1);

        let metric2 = Metric::new(
            "load1",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 1.1 },
        );
        let insert2 = metric_to_insert_request(metric2, &options);
        assert_eq!(insert2.table_name, "load1");
    }

    #[test]
    fn test_counter() {
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 1.1 },
        );
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(rows.rows[0].values.len(), 2);

        assert_eq!(get_column(&rows, LEGACY_VALUE_COLUMN_NAME), 1.1);
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
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(rows.rows[0].values.len(), 2);

        assert_eq!(get_column(&rows, LEGACY_VALUE_COLUMN_NAME), 2.0);
    }

    #[test]
    fn test_distribution() {
        let metric = Metric::new(
            "cpu_seconds_total",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vector_lib::samples![1.0 => 2, 2.0 => 4, 3.0 => 2],
                statistic: StatisticKind::Histogram,
            },
        );
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(
            rows.rows[0].values.len(),
            1 + DISTRIBUTION_STAT_FIELD_COUNT + DISTRIBUTION_QUANTILES.len()
        );

        assert_eq!(get_column(&rows, "max"), 3.0);
        assert_eq!(get_column(&rows, "min"), 1.0);
        assert_eq!(get_column(&rows, "avg"), 2.0);
        assert_eq!(get_column(&rows, "sum"), 16.0);
        assert_eq!(get_column(&rows, "count"), 8.0);
        assert_eq!(get_column(&rows, "p50"), 2.0);
        assert_eq!(get_column(&rows, "p75"), 2.0);
        assert_eq!(get_column(&rows, "p90"), 3.0);
        assert_eq!(get_column(&rows, "p95"), 3.0);
        assert_eq!(get_column(&rows, "p99"), 3.0);
    }

    #[test]
    fn test_histogram() {
        let buckets = vector_lib::buckets![1.0 => 1, 2.0 => 2, 3.0 => 1];
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
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(
            rows.rows[0].values.len(),
            1 + SUMMARY_STAT_FIELD_COUNT + buckets_len
        );

        assert_eq!(get_column(&rows, "b1"), 1.0);
        assert_eq!(get_column(&rows, "b2"), 2.0);
        assert_eq!(get_column(&rows, "b3"), 1.0);
        assert_eq!(get_column(&rows, "count"), 4.0);
        assert_eq!(get_column(&rows, "sum"), 8.0);
    }

    #[test]
    fn test_summary() {
        let quantiles = vector_lib::quantiles![0.01 => 1.5, 0.5 => 2.0, 0.99 => 3.0];
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
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(
            rows.rows[0].values.len(),
            1 + SUMMARY_STAT_FIELD_COUNT + quantiles_len
        );

        assert_eq!(get_column(&rows, "p01"), 1.5);
        assert_eq!(get_column(&rows, "p50"), 2.0);
        assert_eq!(get_column(&rows, "p99"), 3.0);
        assert_eq!(get_column(&rows, "count"), 6.0);
        assert_eq!(get_column(&rows, "sum"), 12.0);
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
        let options = RequestBuilderOptions {
            use_new_naming: false,
        };

        let insert = metric_to_insert_request(metric, &options);
        let rows = insert.rows.expect("Empty insert request");
        assert_eq!(
            rows.rows[0].values.len(),
            1 + DISTRIBUTION_QUANTILES.len() + DISTRIBUTION_STAT_FIELD_COUNT
        );

        assert!(get_column(&rows, "p50") <= 4.0);
        assert!(get_column(&rows, "p95") > 8.0);
        assert!(get_column(&rows, "p95") <= 9.0);
        assert!(get_column(&rows, "p99") > 8.0);
        assert!(get_column(&rows, "p99") <= 9.0);
        assert_eq!(get_column(&rows, "count"), samples as f64);
        assert_eq!(get_column(&rows, "sum"), 45.0);
        assert_eq!(get_column(&rows, "max"), 9.0);
        assert_eq!(get_column(&rows, "min"), 0.0);
    }
}
