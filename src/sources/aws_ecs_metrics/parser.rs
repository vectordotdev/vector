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
        format!("{prefix}_{name}"),
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
        format!("{prefix}_{name}"),
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

    if let Some(cpu_usage) = &cpu.cpu_usage
        && let (Some(percpu_usage), Some(online_cpus)) = (&cpu_usage.percpu_usage, cpu.online_cpus)
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

#[allow(clippy::large_enum_variant)]
#[derive(Deserialize)]
#[serde(untagged, deny_unknown_fields)]
enum StatsPayload {
    Container(ContainerStats),
    Empty {},
    Null,
}

pub(super) fn parse(
    bytes: &[u8],
    namespace: Option<String>,
) -> Result<Vec<Metric>, serde_json::Error> {
    let mut metrics = Vec::new();
    let parsed = serde_json::from_slice::<BTreeMap<String, StatsPayload>>(bytes)?;

    for (id, payload) in parsed {
        let container = match payload {
            StatsPayload::Container(container) => container,
            _ => continue,
        };

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
mod test;
