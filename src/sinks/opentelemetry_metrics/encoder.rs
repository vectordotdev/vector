use std::time::{SystemTime, UNIX_EPOCH};

use vector_lib::opentelemetry::proto::{
    common::v1::{any_value, AnyValue, KeyValue},
    metrics::v1::{
        metric, number_data_point, AggregationTemporality, Gauge, Histogram, HistogramDataPoint,
        Metric, NumberDataPoint, Sum,
    },
};

use crate::event::metric::{Metric as VectorMetric, MetricTags, MetricValue};

use super::config::AggregationTemporalityConfig;

pub fn tags_to_attributes(tags: &MetricTags) -> Vec<KeyValue> {
    tags.iter_single()
        .map(|(k, v)| KeyValue {
            key: k.to_string(),
            value: Some(AnyValue {
                value: Some(any_value::Value::StringValue(v.to_string())),
            }),
        })
        .collect()
}

pub fn encode_metrics(
    events: Vec<VectorMetric>,
    aggregation_temporality: AggregationTemporalityConfig,
) -> Vec<Metric> {
    events
        .into_iter()
        .filter_map(|event| {
            let metric_name = event.name().to_string();
            let timestamp = event
                .timestamp()
                .map(|x| x.timestamp_nanos_opt().unwrap_or(0) as u64);
            let attributes = event.tags().map(tags_to_attributes).unwrap_or_default();

            // Convert Vector metrics to OpenTelemetry metrics
            match event.value() {
                MetricValue::Counter { value } => {
                    let data_point = NumberDataPoint {
                        attributes,
                        time_unix_nano: timestamp.unwrap_or_else(|| {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        }),
                        start_time_unix_nano: 0, // We don't have start time in Vector metrics
                        value: Some(number_data_point::Value::AsDouble(*value)),
                        exemplars: Vec::new(),
                        flags: 0,
                    };

                    let aggregation_temporality = match aggregation_temporality {
                        AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                        AggregationTemporalityConfig::Cumulative => {
                            AggregationTemporality::Cumulative
                        }
                    };

                    Some(Metric {
                        name: metric_name,
                        description: String::new(),
                        unit: String::new(),
                        data: Some(metric::Data::Sum(Sum {
                            data_points: vec![data_point],
                            aggregation_temporality: aggregation_temporality as i32,
                            is_monotonic: true,
                        })),
                    })
                }
                MetricValue::Gauge { value } => {
                    let data_point = NumberDataPoint {
                        attributes,
                        time_unix_nano: timestamp.unwrap_or_else(|| {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        }),
                        start_time_unix_nano: 0, // Not needed for gauges
                        value: Some(number_data_point::Value::AsDouble(*value)),
                        exemplars: Vec::new(),
                        flags: 0,
                    };

                    Some(Metric {
                        name: metric_name,
                        description: String::new(),
                        unit: String::new(),
                        data: Some(metric::Data::Gauge(Gauge {
                            data_points: vec![data_point],
                        })),
                    })
                }
                MetricValue::Distribution {
                    samples,
                    statistic: _,
                } => {
                    // Convert to histogram
                    let mut sum = 0.0;
                    let mut count = 0;
                    let mut bucket_counts = Vec::new();
                    let mut explicit_bounds = Vec::new();

                    // Simple conversion - this could be improved with better bucket boundaries
                    for sample in samples {
                        sum += sample.value * sample.rate as f64;
                        count += sample.rate;

                        // Add a bucket for each sample
                        explicit_bounds.push(sample.value);
                        bucket_counts.push(sample.rate as u64);
                    }

                    // Add the final bucket (infinity)
                    bucket_counts.push(0);

                    let data_point = HistogramDataPoint {
                        attributes,
                        time_unix_nano: timestamp.unwrap_or_else(|| {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        }),
                        start_time_unix_nano: 0,
                        count: count as u64,
                        sum: Some(sum),
                        bucket_counts,
                        explicit_bounds,
                        exemplars: Vec::new(),
                        flags: 0,
                        min: None,
                        max: None,
                    };

                    let aggregation_temporality = match aggregation_temporality {
                        AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                        AggregationTemporalityConfig::Cumulative => {
                            AggregationTemporality::Cumulative
                        }
                    };

                    Some(Metric {
                        name: metric_name,
                        description: String::new(),
                        unit: String::new(),
                        data: Some(metric::Data::Histogram(Histogram {
                            data_points: vec![data_point],
                            aggregation_temporality: aggregation_temporality as i32,
                        })),
                    })
                }
                MetricValue::Set { values } => {
                    // Convert to a sum with the count of unique values
                    let data_point = NumberDataPoint {
                        attributes,
                        time_unix_nano: timestamp.unwrap_or_else(|| {
                            SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_nanos() as u64
                        }),
                        start_time_unix_nano: 0,
                        value: Some(number_data_point::Value::AsDouble(values.len() as f64)),
                        exemplars: Vec::new(),
                        flags: 0,
                    };

                    let aggregation_temporality = match aggregation_temporality {
                        AggregationTemporalityConfig::Delta => AggregationTemporality::Delta,
                        AggregationTemporalityConfig::Cumulative => {
                            AggregationTemporality::Cumulative
                        }
                    };

                    Some(Metric {
                        name: metric_name,
                        description: String::new(),
                        unit: String::new(),
                        data: Some(metric::Data::Sum(Sum {
                            data_points: vec![data_point],
                            aggregation_temporality: aggregation_temporality as i32,
                            is_monotonic: false,
                        })),
                    })
                }
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::metric::{MetricKind, Sample, StatisticKind};
    use chrono::{TimeZone, Utc};

    #[test]
    fn test_encode_counter() {
        let timestamp = Utc.timestamp_nanos(1_612_160_827_000_000_000);
        let mut tags = MetricTags::default();
        tags.insert("host".to_string(), "localhost".to_string());

        let metric = VectorMetric::new(
            "requests_total",
            MetricKind::Incremental,
            MetricValue::Counter { value: 100.0 },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(tags.clone()));

        let encoded = encode_metrics(vec![metric], AggregationTemporalityConfig::Cumulative);
        assert_eq!(encoded.len(), 1);

        let encoded_metric = &encoded[0];
        assert_eq!(encoded_metric.name, "requests_total");

        if let Some(metric::Data::Sum(sum)) = &encoded_metric.data {
            assert_eq!(
                sum.aggregation_temporality,
                AggregationTemporality::Cumulative as i32
            );
            assert!(sum.is_monotonic);
            assert_eq!(sum.data_points.len(), 1);

            let point = &sum.data_points[0];
            assert_eq!(point.time_unix_nano, 1_612_160_827_000_000_000);
            assert_eq!(point.attributes.len(), 1);
            assert_eq!(point.attributes[0].key, "host");
            if let Some(AnyValue {
                value: Some(any_value::Value::StringValue(v)),
            }) = &point.attributes[0].value
            {
                assert_eq!(v, "localhost");
            } else {
                panic!("Unexpected attribute value type");
            }
            if let Some(number_data_point::Value::AsDouble(v)) = point.value {
                assert_eq!(v, 100.0);
            } else {
                panic!("Unexpected data point value type");
            }
        } else {
            panic!("Expected Sum metric data");
        }
    }

    #[test]
    fn test_encode_histogram() {
        let timestamp = Utc.timestamp_nanos(1_612_160_827_000_000_000);
        let mut tags = MetricTags::default();
        tags.insert("host".to_string(), "localhost".to_string());

        let metric = VectorMetric::new(
            "request_duration_ms",
            MetricKind::Incremental,
            MetricValue::Distribution {
                samples: vec![
                    Sample {
                        value: 10.0,
                        rate: 1,
                    },
                    Sample {
                        value: 20.0,
                        rate: 2,
                    },
                    Sample {
                        value: 30.0,
                        rate: 3,
                    },
                ],
                statistic: StatisticKind::Histogram,
            },
        )
        .with_timestamp(Some(timestamp))
        .with_tags(Some(tags.clone()));

        let encoded = encode_metrics(vec![metric], AggregationTemporalityConfig::Delta);
        assert_eq!(encoded.len(), 1);

        let encoded_metric = &encoded[0];
        assert_eq!(encoded_metric.name, "request_duration_ms");

        if let Some(metric::Data::Histogram(histogram)) = &encoded_metric.data {
            assert_eq!(
                histogram.aggregation_temporality,
                AggregationTemporality::Delta as i32
            );
            assert_eq!(histogram.data_points.len(), 1);

            let point = &histogram.data_points[0];
            assert_eq!(point.time_unix_nano, 1_612_160_827_000_000_000);
            assert_eq!(point.count, 6);
            assert_eq!(point.sum, Some(140.0));
            assert_eq!(point.bucket_counts, vec![1, 2, 3, 0]);
            assert_eq!(point.explicit_bounds, vec![10.0, 20.0, 30.0]);
        } else {
            panic!("Expected Histogram metric data");
        }
    }
}
