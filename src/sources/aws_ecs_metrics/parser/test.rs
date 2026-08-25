use chrono::{DateTime, Timelike, Utc, offset::TimeZone};
use vector_lib::{assert_event_data_eq, metric_tags};

use super::parse;
use crate::event::metric::{Metric, MetricKind, MetricValue};

fn ts() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2018, 11, 14, 8, 9, 10)
        .single()
        .and_then(|t| t.with_nanosecond(11))
        .expect("invalid timestamp")
}

fn namespace() -> String {
    "aws_ecs".into()
}

#[test]
fn parse_block_io_metrics() {
    let json = r#"
    {
        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
            "read": "2018-11-14T08:09:10.000000011Z",
            "name": "vector2",
            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "blkio_stats": {
                "io_service_bytes_recursive": [
                    {
                        "major": 202,
                        "minor": 26368,
                        "op": "Read",
                        "value": 0
                    },
                    {
                        "major": 202,
                        "minor": 26368,
                        "op": "Write",
                        "value": 520192
                    }
                ],
                "io_serviced_recursive": [],
                "io_queue_recursive": [],
                "io_service_time_recursive": [],
                "io_wait_time_recursive": [],
                "io_merged_recursive": [],
                "io_time_recursive": [],
                "sectors_recursive": []
            }
        },
        "123456789": {},
        "123456789": null
    }"#;

    assert_event_data_eq!(
        parse(json.as_bytes(), Some(namespace())).unwrap(),
        vec![
            Metric::new(
                "blkio_recursive_io_service_bytes_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "device" => "202:26368",
                "op" => "read",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "blkio_recursive_io_service_bytes_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 520192.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "device" => "202:26368",
                "op" => "write",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
        ],
    );
}

#[test]
fn parse_cpu_metrics() {
    let json = r#"
    {
        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
            "read": "2018-11-14T08:09:10.000000011Z",
            "name": "vector2",
            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "cpu_stats": {
                "cpu_usage": {
                    "total_usage": 2324920942,
                    "percpu_usage": [
                        1095931487,
                        1228989455,
                        0,
                        0
                    ],
                    "usage_in_kernelmode": 190000000,
                    "usage_in_usermode": 510000000
                },
                "system_cpu_usage": 2007130000000,
                "online_cpus": 2,
                "throttling_data": {
                    "periods": 0,
                    "throttled_periods": 0,
                    "throttled_time": 0
                }
            }
        },
        "2344": {},
        "test": null
    }"#;

    assert_event_data_eq!(
        parse(json.as_bytes(), Some(namespace())).unwrap(),
        vec![
            Metric::new(
                "cpu_online_cpus",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 2.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2"
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_system_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 2007130000000.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_usermode_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 510000000.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_kernelmode_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 190000000.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_total_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 2324920942.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_throttling_periods_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_throttled_periods_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_throttled_time_seconds_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_percpu_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 1095931487.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "cpu" => "0",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "cpu_usage_percpu_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 1228989455.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "cpu" => "1",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
        ],
    );
}

#[test]
fn parse_precpu_metrics() {
    let json = r#"
    {
        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
            "read": "2018-11-14T08:09:10.000000011Z",
            "name": "vector2",
            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "precpu_stats": {
                "cpu_usage": {
                    "total_usage": 2324920942,
                    "percpu_usage": [
                        1095931487,
                        1228989455,
                        0,
                        0
                    ],
                    "usage_in_kernelmode": 190000000,
                    "usage_in_usermode": 510000000
                },
                "system_cpu_usage": 2007130000000,
                "online_cpus": 2,
                "throttling_data": {
                    "periods": 0,
                    "throttled_periods": 0,
                    "throttled_time": 0
                }
            }
        }
    }"#;

    assert_event_data_eq!(
        parse(json.as_bytes(), Some(namespace())).unwrap(),
        vec![
            Metric::new(
                "precpu_online_cpus",
                MetricKind::Absolute,
                MetricValue::Gauge { value: 2.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2"
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_system_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 2007130000000.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_usermode_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 510000000.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_kernelmode_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 190000000.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_total_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 2324920942.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_throttling_periods_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_throttled_periods_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_throttled_time_seconds_total",
                MetricKind::Absolute,
                MetricValue::Counter { value: 0.0 },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_percpu_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 1095931487.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "cpu" => "0",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
            Metric::new(
                "precpu_usage_percpu_jiffies_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: 1228989455.0
                },
            )
            .with_namespace(Some(namespace()))
            .with_tags(Some(metric_tags!(
                "cpu" => "1",
                "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
                "container_name" => "vector2",
            )))
            .with_timestamp(Some(ts())),
        ],
    );
}

#[test]
fn parse_memory_metrics() {
    let json = r#"
    {
        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
            "read": "2018-11-14T08:09:10.000000011Z",
            "name": "vector2",
            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "memory_stats": {
                "usage": 40120320,
                "max_usage": 47177728,
                "stats": {
                    "active_anon": 34885632,
                    "active_file": 65536,
                    "cache": 413696,
                    "dirty": 0,
                    "hierarchical_memory_limit": 536870912,
                    "hierarchical_memsw_limit": 9223372036854771712,
                    "inactive_anon": 4096,
                    "inactive_file": 344064,
                    "mapped_file": 4096,
                    "pgfault": 31131,
                    "pgmajfault": 0,
                    "pgpgin": 22360,
                    "pgpgout": 13742,
                    "rss": 34885632,
                    "rss_huge": 0,
                    "total_active_anon": 34885632,
                    "total_active_file": 65536,
                    "total_cache": 413696,
                    "total_dirty": 0,
                    "total_inactive_anon": 4096,
                    "total_inactive_file": 344064,
                    "total_mapped_file": 4096,
                    "total_pgfault": 31131,
                    "total_pgmajfault": 0,
                    "total_pgpgin": 22360,
                    "total_pgpgout": 13742,
                    "total_rss": 34885632,
                    "total_rss_huge": 0,
                    "total_unevictable": 0,
                    "total_writeback": 0,
                    "unevictable": 0,
                    "writeback": 0
                },
                "limit": 9223372036854771712
            }
        }
    }"#;

    let metrics = parse(json.as_bytes(), Some(namespace())).unwrap();

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "memory_used_bytes")
            .unwrap(),
        &Metric::new(
            "memory_used_bytes",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 40120320.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "memory_max_used_bytes")
            .unwrap(),
        &Metric::new(
            "memory_max_used_bytes",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 47177728.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "memory_active_anonymous_bytes")
            .unwrap(),
        &Metric::new(
            "memory_active_anonymous_bytes",
            MetricKind::Absolute,
            MetricValue::Gauge { value: 34885632.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "memory_total_page_faults_total")
            .unwrap(),
        &Metric::new(
            "memory_total_page_faults_total",
            MetricKind::Absolute,
            MetricValue::Counter { value: 31131.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );
}

#[test]
fn parse_network_metrics() {
    let json = r#"
    {
        "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352": {
            "read": "2018-11-14T08:09:10.000000011Z",
            "name": "vector2",
            "id": "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "networks": {
                "eth1": {
                    "rx_bytes": 329932716,
                    "rx_packets": 224158,
                    "rx_errors": 0,
                    "rx_dropped": 0,
                    "tx_bytes": 2001229,
                    "tx_packets": 29201,
                    "tx_errors": 0,
                    "tx_dropped": 0
                }
            }
        }
    }"#;

    let metrics = parse(json.as_bytes(), Some(namespace())).unwrap();

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "network_receive_bytes_total")
            .unwrap(),
        &Metric::new(
            "network_receive_bytes_total",
            MetricKind::Absolute,
            MetricValue::Counter { value: 329932716.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "device" => "eth1",
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );

    assert_event_data_eq!(
        metrics
            .iter()
            .find(|m| m.name() == "network_transmit_bytes_total")
            .unwrap(),
        &Metric::new(
            "network_transmit_bytes_total",
            MetricKind::Absolute,
            MetricValue::Counter { value: 2001229.0 },
        )
        .with_namespace(Some(namespace()))
        .with_tags(Some(metric_tags!(
            "device" => "eth1",
            "container_id" => "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352",
            "container_name" => "vector2",
        )))
        .with_timestamp(Some(ts())),
    );
}
