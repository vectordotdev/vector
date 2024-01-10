use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::event::metric::{Metric, MetricKind, MetricTags, MetricValue};

#[derive(Deserialize)]
struct BlockIoStat {
    major: usize,
    minor: usize,
    op: String,
    value: f64,
}

#[derive(Deserialize)]
struct BlockIoStats {
    io_merged_recursive: Option<Vec<BlockIoStat>>,
    io_queue_recursive: Option<Vec<BlockIoStat>>,
    io_service_bytes_recursive: Option<Vec<BlockIoStat>>,
    io_service_time_recursive: Option<Vec<BlockIoStat>>,
    io_serviced_recursive: Option<Vec<BlockIoStat>>,
    io_time_recursive: Option<Vec<BlockIoStat>>,
    io_wait_time_recursive: Option<Vec<BlockIoStat>>,
    sectors_recursive: Option<Vec<BlockIoStat>>,
}

#[derive(Deserialize)]
struct CpuUsage {
    total_usage: Option<f64>,
    percpu_usage: Option<Vec<f64>>,
    usage_in_usermode: Option<f64>,
    usage_in_kernelmode: Option<f64>,
}

#[derive(Deserialize)]
struct ThrottlingData {
    periods: Option<f64>,
    throttled_periods: Option<f64>,
    throttled_time: Option<f64>,
}

#[derive(Deserialize)]
struct CpuStats {
    cpu_usage: Option<CpuUsage>,
    system_cpu_usage: Option<f64>,
    online_cpus: Option<usize>,
    throttling_data: Option<ThrottlingData>,
}

#[derive(Deserialize)]
struct MemoryExtStats {
    active_anon: Option<f64>,
    active_file: Option<f64>,
    cache: Option<f64>,
    dirty: Option<f64>,
    inactive_anon: Option<f64>,
    inactive_file: Option<f64>,
    mapped_file: Option<f64>,
    pgfault: Option<f64>,
    pgmajfault: Option<f64>,
    pgpgin: Option<f64>,
    pgpgout: Option<f64>,
    rss: Option<f64>,
    rss_huge: Option<f64>,
    unevictable: Option<f64>,
    writeback: Option<f64>,
    total_active_anon: Option<f64>,
    total_active_file: Option<f64>,
    total_cache: Option<f64>,
    total_dirty: Option<f64>,
    total_inactive_anon: Option<f64>,
    total_inactive_file: Option<f64>,
    total_mapped_file: Option<f64>,
    total_pgfault: Option<f64>,
    total_pgmajfault: Option<f64>,
    total_pgpgin: Option<f64>,
    total_pgpgout: Option<f64>,
    total_rss: Option<f64>,
    total_rss_huge: Option<f64>,
    total_unevictable: Option<f64>,
    total_writeback: Option<f64>,
    hierarchical_memory_limit: Option<f64>,
    hierarchical_memsw_limit: Option<f64>,
}

#[derive(Deserialize)]
struct MemoryStats {
    usage: Option<f64>,
    max_usage: Option<f64>,
    limit: Option<f64>,
    stats: Option<MemoryExtStats>,
}

#[derive(Deserialize)]
struct NetworkStats {
    rx_bytes: Option<f64>,
    rx_packets: Option<f64>,
    rx_errors: Option<f64>,
    rx_dropped: Option<f64>,
    tx_bytes: Option<f64>,
    tx_packets: Option<f64>,
    tx_errors: Option<f64>,
    tx_dropped: Option<f64>,
}

#[derive(Deserialize)]
struct ContainerStats {
    #[serde(rename = "read")]
    ts: DateTime<Utc>,
    name: Option<String>,
    blkio_stats: Option<BlockIoStats>,
    cpu_stats: Option<CpuStats>,
    precpu_stats: Option<CpuStats>,
    memory_stats: Option<MemoryStats>,
    #[serde(default)]
    networks: Option<BTreeMap<String, NetworkStats>>,
}

fn counter(
    prefix: &str,
    name: &str,
    namespace: Option<String>,
    timestamp: DateTime<Utc>,
    value: f64,
    tags: MetricTags,
) -> Metric {
    Metric::new(
        format!("{}_{}", prefix, name),
        MetricKind::Absolute,
        MetricValue::Counter { value },
    )
    .with_namespace(namespace)
    .with_tags(Some(tags))
    .with_timestamp(Some(timestamp))
}

fn gauge(
    prefix: &str,
    name: &str,
    namespace: Option<String>,
    timestamp: DateTime<Utc>,
    value: f64,
    tags: MetricTags,
) -> Metric {
    Metric::new(
        format!("{}_{}", prefix, name),
        MetricKind::Absolute,
        MetricValue::Gauge { value },
    )
    .with_namespace(namespace)
    .with_tags(Some(tags))
    .with_timestamp(Some(timestamp))
}

fn blkio_tags(item: &BlockIoStat, tags: &MetricTags) -> MetricTags {
    let mut tags = tags.clone();
    tags.replace("device".into(), format!("{}:{}", item.major, item.minor));
    tags.replace("op".into(), item.op.to_lowercase());
    tags
}

/// reference <https://www.kernel.org/doc/Documentation/cgroup-v1/blkio-controller.txt>
fn blkio_metrics(
    blkio: &BlockIoStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &MetricTags,
) -> Vec<Metric> {
    let mut metrics = vec![];

    metrics.extend(blkio.io_merged_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_merged_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_queue_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_queued_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_service_bytes_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_service_bytes_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_service_time_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_service_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000_000_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_serviced_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_serviced_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_time_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_wait_time_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_io_wait_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000_000_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.sectors_recursive.iter().flatten().map(|s| {
        counter(
            "blkio",
            "recursive_sectors_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));

    metrics
}

fn cpu_metrics(
    cpu: &CpuStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &MetricTags,
    usage: &str,
) -> Vec<Metric> {
    // Eight expected metrics not including online_cpus
    let size = 8 + cpu.online_cpus.unwrap_or(0);
    let mut metrics = Vec::with_capacity(size);

    if let Some(online_cpus) = cpu.online_cpus {
        metrics.push(gauge(
            usage,
            "online_cpus",
            namespace.clone(),
            timestamp,
            online_cpus as f64,
            tags.clone(),
        ));
    }

    if let Some(system_cpu_usage) = cpu.system_cpu_usage {
        metrics.push(counter(
            usage,
            "usage_system_jiffies_total",
            namespace.clone(),
            timestamp,
            system_cpu_usage,
            tags.clone(),
        ));
    }

    if let Some(cpu_usage) = &cpu.cpu_usage {
        metrics.extend(
            [
                ("usage_usermode_jiffies_total", cpu_usage.usage_in_usermode),
                (
                    "usage_kernelmode_jiffies_total",
                    cpu_usage.usage_in_kernelmode,
                ),
                ("usage_total_jiffies_total", cpu_usage.total_usage),
            ]
            .iter()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    counter(
                        usage,
                        name,
                        namespace.clone(),
                        timestamp,
                        value,
                        tags.clone(),
                    )
                })
            }),
        );
    }

    if let Some(throttling_data) = &cpu.throttling_data {
        metrics.extend(
            [
                ("throttling_periods_total", throttling_data.periods),
                ("throttled_periods_total", throttling_data.throttled_periods),
                (
                    "throttled_time_seconds_total",
                    throttling_data
                        .throttled_time
                        .map(|throttled_time| throttled_time / 1_000_000_000.0),
                ),
            ]
            .iter()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    counter(
                        usage,
                        name,
                        namespace.clone(),
                        timestamp,
                        value,
                        tags.clone(),
                    )
                })
            }),
        );
    }

    if let Some(cpu_usage) = &cpu.cpu_usage {
        if let (Some(percpu_usage), Some(online_cpus)) = (&cpu_usage.percpu_usage, cpu.online_cpus)
        {
            metrics.extend((0..online_cpus).filter_map(|index| {
                percpu_usage.get(index).map(|value| {
                    let mut tags = tags.clone();
                    tags.replace("cpu".into(), index.to_string());

                    counter(
                        usage,
                        "usage_percpu_jiffies_total",
                        namespace.clone(),
                        timestamp,
                        *value,
                        tags,
                    )
                })
            }));
        }
    }

    metrics
}

fn memory_metrics(
    memory: &MemoryStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &MetricTags,
) -> Vec<Metric> {
    let mut metrics = Vec::with_capacity(35);

    metrics.extend(
        [
            ("used_bytes", memory.usage),
            ("max_used_bytes", memory.max_usage),
            ("limit_bytes", memory.limit),
        ]
        .iter()
        .filter_map(|(name, value)| {
            value.map(|value| {
                gauge(
                    "memory",
                    name,
                    namespace.clone(),
                    timestamp,
                    value,
                    tags.clone(),
                )
            })
        }),
    );

    if let Some(stats) = &memory.stats {
        metrics.extend(
            [
                ("active_anonymous_bytes", stats.active_anon),
                ("active_file_bytes", stats.active_file),
                ("cache_bytes", stats.cache),
                ("dirty_bytes", stats.dirty),
                ("inactive_anonymous_bytes", stats.inactive_anon),
                ("inactive_file_bytes", stats.inactive_file),
                ("mapped_file_bytes", stats.mapped_file),
                ("rss_bytes", stats.rss),
                ("rss_hugepages_bytes", stats.rss_huge),
                ("unevictable_bytes", stats.unevictable),
                ("writeback_bytes", stats.writeback),
                ("total_active_anonymous_bytes", stats.total_active_anon),
                ("total_active_file_bytes", stats.total_active_file),
                ("total_cache_bytes", stats.total_cache),
                ("total_dirty_bytes", stats.total_dirty),
                ("total_inactive_anonymous_bytes", stats.total_inactive_anon),
                ("total_inactive_file_bytes", stats.total_inactive_file),
                ("total_mapped_file_bytes", stats.total_mapped_file),
                ("total_rss_bytes", stats.total_rss),
                ("total_rss_hugepages_bytes", stats.total_rss_huge),
                ("total_unevictable_bytes", stats.total_unevictable),
                ("total_writeback_bytes", stats.total_writeback),
                (
                    "hierarchical_memory_limit_bytes",
                    stats.hierarchical_memory_limit,
                ),
                (
                    "hierarchical_memsw_limit_bytes",
                    stats.hierarchical_memsw_limit,
                ),
            ]
            .iter()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    gauge(
                        "memory",
                        name,
                        namespace.clone(),
                        timestamp,
                        value,
                        tags.clone(),
                    )
                })
            }),
        );

        metrics.extend(
            [
                ("page_faults_total", stats.pgfault),
                ("major_faults_total", stats.pgmajfault),
                ("page_charged_total", stats.pgpgin),
                ("page_uncharged_total", stats.pgpgout),
                ("total_page_faults_total", stats.total_pgfault),
                ("total_major_faults_total", stats.total_pgmajfault),
                ("total_page_charged_total", stats.total_pgpgin),
                ("total_page_uncharged_total", stats.total_pgpgout),
            ]
            .iter()
            .filter_map(|(name, value)| {
                value.map(|value| {
                    counter(
                        "memory",
                        name,
                        namespace.clone(),
                        timestamp,
                        value,
                        tags.clone(),
                    )
                })
            }),
        );
    }

    metrics
}

fn network_metrics(
    interface: &str,
    network: &NetworkStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &MetricTags,
) -> Vec<Metric> {
    let mut tags = tags.clone();
    tags.replace("device".into(), interface.to_string());

    [
        ("receive_bytes_total", network.rx_bytes),
        ("receive_packets_total", network.rx_packets),
        ("receive_packets_drop_total", network.rx_dropped),
        ("receive_errs_total", network.rx_errors),
        ("transmit_bytes_total", network.tx_bytes),
        ("transmit_packets_total", network.tx_packets),
        ("transmit_packets_drop_total", network.tx_dropped),
        ("transmit_errs_total", network.tx_errors),
    ]
    .iter()
    .filter(|(_name, value)| value.is_some())
    .map(|(name, value)| {
        counter(
            "network",
            name,
            namespace.clone(),
            timestamp,
            value.unwrap(),
            tags.clone(),
        )
    })
    .collect()
}

pub(super) fn parse(
    bytes: &[u8],
    namespace: Option<String>,
) -> Result<Vec<Metric>, serde_json::Error> {
    let mut metrics = Vec::new();
    let parsed = serde_json::from_slice::<BTreeMap<String, ContainerStats>>(bytes)?;

    for (id, container) in parsed {
        let mut tags = MetricTags::default();
        tags.replace("container_id".into(), id);
        if let Some(name) = container.name {
            tags.replace("container_name".into(), name);
        }

        if let Some(blkio) = container.blkio_stats {
            metrics.extend(blkio_metrics(&blkio, container.ts, &namespace, &tags));
        }

        if let Some(cpu) = container.cpu_stats {
            metrics.extend(cpu_metrics(&cpu, container.ts, &namespace, &tags, "cpu"));
        }

        if let Some(precpu) = container.precpu_stats {
            metrics.extend(cpu_metrics(
                &precpu,
                container.ts,
                &namespace,
                &tags,
                "precpu",
            ));
        }

        if let Some(memory) = container.memory_stats {
            metrics.extend(memory_metrics(&memory, container.ts, &namespace, &tags));
        }

        for (interface, network) in container.networks.iter().flatten() {
            metrics.extend(network_metrics(
                interface,
                network,
                container.ts,
                &namespace,
                &tags,
            ));
        }
    }

    Ok(metrics)
}

#[cfg(test)]
mod test {
    use chrono::{offset::TimeZone, DateTime, Timelike, Utc};
    use vector_lib::assert_event_data_eq;
    use vector_lib::metric_tags;

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
            }
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
            }
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
}
