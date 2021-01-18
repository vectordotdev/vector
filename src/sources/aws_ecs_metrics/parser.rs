use crate::event::metric::{Metric, MetricKind, MetricValue};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
struct BlockIoStat {
    major: usize,
    minor: usize,
    op: String,
    value: f64,
}

#[derive(Deserialize)]
struct BlockIoStats {
    io_merged_recursive: Vec<BlockIoStat>,
    io_queue_recursive: Vec<BlockIoStat>,
    io_service_bytes_recursive: Vec<BlockIoStat>,
    io_service_time_recursive: Vec<BlockIoStat>,
    io_serviced_recursive: Vec<BlockIoStat>,
    io_time_recursive: Vec<BlockIoStat>,
    io_wait_time_recursive: Vec<BlockIoStat>,
    sectors_recursive: Vec<BlockIoStat>,
}

#[derive(Deserialize)]
struct CpuUsage {
    total_usage: f64,
    percpu_usage: Vec<f64>,
    usage_in_usermode: f64,
    usage_in_kernelmode: f64,
}

#[derive(Deserialize)]
struct ThrottlingData {
    periods: f64,
    throttled_periods: f64,
    throttled_time: f64,
}

#[derive(Deserialize)]
struct CpuStats {
    cpu_usage: CpuUsage,
    system_cpu_usage: f64,
    online_cpus: usize,
    throttling_data: ThrottlingData,
}

#[derive(Deserialize)]
struct MemoryExtStats {
    active_anon: f64,
    active_file: f64,
    cache: f64,
    dirty: f64,
    inactive_anon: f64,
    inactive_file: f64,
    mapped_file: f64,
    pgfault: f64,
    pgmajfault: f64,
    pgpgin: f64,
    pgpgout: f64,
    rss: f64,
    rss_huge: f64,
    unevictable: f64,
    writeback: f64,
    total_active_anon: f64,
    total_active_file: f64,
    total_cache: f64,
    total_dirty: f64,
    total_inactive_anon: f64,
    total_inactive_file: f64,
    total_mapped_file: f64,
    total_pgfault: f64,
    total_pgmajfault: f64,
    total_pgpgin: f64,
    total_pgpgout: f64,
    total_rss: f64,
    total_rss_huge: f64,
    total_unevictable: f64,
    total_writeback: f64,
    hierarchical_memory_limit: f64,
    hierarchical_memsw_limit: f64,
}

#[derive(Deserialize)]
struct MemoryStats {
    usage: f64,
    max_usage: f64,
    limit: f64,
    stats: MemoryExtStats,
}

#[derive(Deserialize)]
struct NetworkStats {
    rx_bytes: f64,
    rx_packets: f64,
    rx_errors: f64,
    rx_dropped: f64,
    tx_bytes: f64,
    tx_packets: f64,
    tx_errors: f64,
    tx_dropped: f64,
}

#[derive(Deserialize)]
struct ContainerStats {
    #[serde(rename = "read")]
    ts: DateTime<Utc>,
    name: Option<String>,
    blkio_stats: Option<BlockIoStats>,
    cpu_stats: Option<CpuStats>,
    memory_stats: Option<MemoryStats>,
    #[serde(default)]
    networks: BTreeMap<String, NetworkStats>,
}

fn counter(
    prefix: &str,
    name: &str,
    namespace: Option<String>,
    timestamp: DateTime<Utc>,
    value: f64,
    tags: BTreeMap<String, String>,
) -> Metric {
    Metric::new(
        format!("{}_{}", prefix, name),
        namespace,
        Some(timestamp),
        Some(tags),
        MetricKind::Absolute,
        MetricValue::Counter { value },
    )
}

fn gauge(
    prefix: &str,
    name: &str,
    namespace: Option<String>,
    timestamp: DateTime<Utc>,
    value: f64,
    tags: BTreeMap<String, String>,
) -> Metric {
    Metric::new(
        format!("{}_{}", prefix, name),
        namespace,
        Some(timestamp),
        Some(tags),
        MetricKind::Absolute,
        MetricValue::Gauge { value },
    )
}

fn blkio_tags(item: &BlockIoStat, tags: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut tags = tags.clone();
    tags.insert("device".into(), format!("{}:{}", item.major, item.minor));
    tags.insert("op".into(), item.op.to_lowercase());
    tags
}

/// reference https://www.kernel.org/doc/Documentation/cgroup-v1/blkio-controller.txt
fn blkio_metrics(
    blkio: &BlockIoStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &BTreeMap<String, String>,
) -> Vec<Metric> {
    let mut metrics = vec![];

    metrics.extend(blkio.io_merged_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_merged_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_queue_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_queued_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_service_bytes_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_service_bytes_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_service_time_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_service_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000_000_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_serviced_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_serviced_total",
            namespace.clone(),
            timestamp,
            s.value,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_time_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.io_wait_time_recursive.iter().map(|s| {
        counter(
            "blkio",
            "recursive_io_wait_time_seconds_total",
            namespace.clone(),
            timestamp,
            s.value / 1_000_000_000.0,
            blkio_tags(s, tags),
        )
    }));
    metrics.extend(blkio.sectors_recursive.iter().map(|s| {
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
    tags: &BTreeMap<String, String>,
) -> Vec<Metric> {
    let mut metrics = vec![];

    metrics.push(gauge(
        "cpu",
        "online_cpus",
        namespace.clone(),
        timestamp,
        cpu.online_cpus as f64,
        tags.clone(),
    ));

    metrics.extend(
        vec![
            ("usage_system_jiffies_total", cpu.system_cpu_usage),
            (
                "usage_usermode_jiffies_total",
                cpu.cpu_usage.usage_in_usermode,
            ),
            (
                "usage_kernelmode_jiffies_total",
                cpu.cpu_usage.usage_in_kernelmode,
            ),
            ("usage_total_jiffies_total", cpu.cpu_usage.total_usage),
            ("throttling_periods_total", cpu.throttling_data.periods),
            (
                "throttled_periods_total",
                cpu.throttling_data.throttled_periods,
            ),
            (
                "throttled_time_seconds_total",
                cpu.throttling_data.throttled_time / 1_000_000_000.0,
            ),
        ]
        .iter()
        .map(|(name, value)| {
            counter(
                "cpu",
                name,
                namespace.clone(),
                timestamp,
                *value,
                tags.clone(),
            )
        }),
    );

    metrics.extend((0..cpu.online_cpus).filter_map(|index| {
        cpu.cpu_usage.percpu_usage.get(index).map(|value| {
            let mut tags = tags.clone();
            tags.insert("cpu".into(), index.to_string());

            counter(
                "cpu",
                "usage_percpu_jiffies_total",
                namespace.clone(),
                timestamp,
                *value,
                tags,
            )
        })
    }));

    metrics
}

fn memory_metrics(
    memory: &MemoryStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &BTreeMap<String, String>,
) -> Vec<Metric> {
    let mut metrics = vec![];

    metrics.extend(
        vec![
            ("used_bytes", memory.usage),
            ("max_used_bytes", memory.max_usage),
            ("limit_bytes", memory.limit),
            ("active_anonymous_bytes", memory.stats.active_anon),
            ("active_file_bytes", memory.stats.active_file),
            ("cache_bytes", memory.stats.cache),
            ("dirty_bytes", memory.stats.dirty),
            ("inactive_anonymous_bytes", memory.stats.inactive_anon),
            ("inactive_file_bytes", memory.stats.inactive_file),
            ("mapped_file_bytes", memory.stats.mapped_file),
            ("rss_bytes", memory.stats.rss),
            ("rss_hugepages_bytes", memory.stats.rss_huge),
            ("unevictable_bytes", memory.stats.unevictable),
            ("writeback_bytes", memory.stats.writeback),
            (
                "total_active_anonymous_bytes",
                memory.stats.total_active_anon,
            ),
            ("total_active_file_bytes", memory.stats.total_active_file),
            ("total_cache_bytes", memory.stats.total_cache),
            ("total_dirty_bytes", memory.stats.total_dirty),
            (
                "total_inactive_anonymous_bytes",
                memory.stats.total_inactive_anon,
            ),
            (
                "total_inactive_file_bytes",
                memory.stats.total_inactive_file,
            ),
            ("total_mapped_file_bytes", memory.stats.total_mapped_file),
            ("total_rss_bytes", memory.stats.total_rss),
            ("total_rss_hugepages_bytes", memory.stats.total_rss_huge),
            ("total_unevictable_bytes", memory.stats.total_unevictable),
            ("total_writeback_bytes", memory.stats.total_writeback),
            (
                "hierarchical_memory_limit_bytes",
                memory.stats.hierarchical_memory_limit,
            ),
            (
                "hierarchical_memsw_limit_bytes",
                memory.stats.hierarchical_memsw_limit,
            ),
        ]
        .iter()
        .map(|(name, value)| {
            gauge(
                "memory",
                name,
                namespace.clone(),
                timestamp,
                *value,
                tags.clone(),
            )
        }),
    );

    metrics.extend(
        vec![
            ("page_faults_total", memory.stats.pgfault),
            ("major_faults_total", memory.stats.pgmajfault),
            ("page_charged_total", memory.stats.pgpgin),
            ("page_uncharged_total", memory.stats.pgpgout),
            ("total_page_faults_total", memory.stats.total_pgfault),
            ("total_major_faults_total", memory.stats.total_pgmajfault),
            ("total_page_charged_total", memory.stats.total_pgpgin),
            ("total_page_uncharged_total", memory.stats.total_pgpgout),
        ]
        .iter()
        .map(|(name, value)| {
            counter(
                "memory",
                name,
                namespace.clone(),
                timestamp,
                *value,
                tags.clone(),
            )
        }),
    );

    metrics
}

fn network_metrics(
    interface: &str,
    network: &NetworkStats,
    timestamp: DateTime<Utc>,
    namespace: &Option<String>,
    tags: &BTreeMap<String, String>,
) -> Vec<Metric> {
    let mut tags = tags.clone();
    tags.insert("device".into(), interface.into());

    vec![
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
    .map(|(name, value)| {
        counter(
            "network",
            name,
            namespace.clone(),
            timestamp,
            *value,
            tags.clone(),
        )
    })
    .collect()
}

pub fn parse(bytes: &[u8], namespace: Option<String>) -> Result<Vec<Metric>, serde_json::Error> {
    let mut metrics = Vec::new();
    let parsed = serde_json::from_slice::<BTreeMap<String, ContainerStats>>(bytes)?;

    for (id, container) in parsed {
        let mut tags = BTreeMap::new();
        tags.insert("container_id".into(), id);
        if let Some(name) = container.name {
            tags.insert("container_name".into(), name);
        }

        if let Some(blkio) = container.blkio_stats {
            metrics.extend(blkio_metrics(&blkio, container.ts, &namespace, &tags));
        }

        if let Some(cpu) = container.cpu_stats {
            metrics.extend(cpu_metrics(&cpu, container.ts, &namespace, &tags));
        }

        if let Some(memory) = container.memory_stats {
            metrics.extend(memory_metrics(&memory, container.ts, &namespace, &tags));
        }

        for (interface, network) in container.networks.iter() {
            metrics.extend(network_metrics(
                &interface,
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
    use super::parse;
    use crate::event::metric::{Metric, MetricKind, MetricValue};
    use chrono::{offset::TimeZone, DateTime, Utc};
    use pretty_assertions::assert_eq;

    fn ts() -> DateTime<Utc> {
        Utc.ymd(2018, 11, 14).and_hms_nano(8, 9, 10, 11)
    }

    fn namespace() -> Option<String> {
        Some("aws_ecs".into())
    }

    #[test]
    fn parse_block_io_metrics() {
        let json = r##"
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
        }"##;

        assert_eq!(
            parse(json.as_bytes(), namespace()).unwrap(),
            vec![
                Metric::new(
                    "blkio_recursive_io_service_bytes_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            ("device".into(), "202:26368".into()),
                            ("op".into(), "read".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "blkio_recursive_io_service_bytes_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            ("device".into(), "202:26368".into()),
                            ("op".into(), "write".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 520192.0 },
                ),
            ],
        );
    }

    #[test]
    fn parse_cpu_metrics() {
        let json = r##"
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
        }"##;

        assert_eq!(
            parse(json.as_bytes(), namespace()).unwrap(),
            vec![
                Metric::new(
                    "cpu_online_cpus".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Gauge { value: 2.0 },
                ),
                Metric::new(
                    "cpu_usage_system_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: 2007130000000.0
                    },
                ),
                Metric::new(
                    "cpu_usage_usermode_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 510000000.0 },
                ),
                Metric::new(
                    "cpu_usage_kernelmode_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 190000000.0 },
                ),
                Metric::new(
                    "cpu_usage_total_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: 2324920942.0
                    },
                ),
                Metric::new(
                    "cpu_throttling_periods_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "cpu_throttled_periods_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "cpu_throttled_time_seconds_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 0.0 },
                ),
                Metric::new(
                    "cpu_usage_percpu_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            ("cpu".into(), "0".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: 1095931487.0
                    },
                ),
                Metric::new(
                    "cpu_usage_percpu_jiffies_total".into(),
                    namespace(),
                    Some(ts()),
                    Some(
                        vec![
                            ("cpu".into(), "1".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    MetricKind::Absolute,
                    MetricValue::Counter {
                        value: 1228989455.0
                    },
                ),
            ],
        );
    }

    #[test]
    fn parse_memory_metrics() {
        let json = r##"
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
        }"##;

        let metrics = parse(json.as_bytes(), namespace()).unwrap();

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "memory_used_bytes")
                .unwrap(),
            &Metric::new(
                "memory_used_bytes".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge { value: 40120320.0 },
            ),
        );

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "memory_max_used_bytes")
                .unwrap(),
            &Metric::new(
                "memory_max_used_bytes".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge { value: 47177728.0 },
            ),
        );

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "memory_active_anonymous_bytes")
                .unwrap(),
            &Metric::new(
                "memory_active_anonymous_bytes".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Gauge { value: 34885632.0 },
            ),
        );

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "memory_total_page_faults_total")
                .unwrap(),
            &Metric::new(
                "memory_total_page_faults_total".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Counter { value: 31131.0 },
            ),
        );
    }

    #[test]
    fn parse_network_metrics() {
        let json = r##"
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
        }"##;

        let metrics = parse(json.as_bytes(), namespace()).unwrap();

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "network_receive_bytes_total")
                .unwrap(),
            &Metric::new(
                "network_receive_bytes_total".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        ("device".into(), "eth1".into()),
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Counter { value: 329932716.0 },
            ),
        );

        assert_eq!(
            metrics
                .iter()
                .find(|m| m.name() == "network_transmit_bytes_total")
                .unwrap(),
            &Metric::new(
                "network_transmit_bytes_total".into(),
                namespace(),
                Some(ts()),
                Some(
                    vec![
                        ("device".into(), "eth1".into()),
                        (
                            "container_id".into(),
                            "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                        ),
                        ("container_name".into(), "vector2".into())
                    ]
                    .into_iter()
                    .collect()
                ),
                MetricKind::Absolute,
                MetricValue::Counter { value: 2001229.0 },
            ),
        );
    }
}
