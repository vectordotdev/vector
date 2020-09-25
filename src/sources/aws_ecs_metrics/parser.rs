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
struct MemoryStats {
    usage: f64,
    max_usage: f64,
    limit: f64,
    stats: BTreeMap<String, f64>,
}

#[derive(Deserialize)]
struct ContainerStats {
    #[serde(rename = "read")]
    ts: DateTime<Utc>,
    name: String,
    id: String,
    #[serde(default, rename = "blkio_stats")]
    block_io: BTreeMap<String, Vec<BlockIoStat>>,
    #[serde(rename = "cpu_stats")]
    cpu: Option<CpuStats>,
    #[serde(rename = "memory_stats")]
    memory: Option<MemoryStats>,
    #[serde(default)]
    networks: BTreeMap<String, BTreeMap<String, f64>>,
}

pub fn parse(packet: &str) -> Result<Vec<Metric>, serde_json::Error> {
    let mut result = Vec::new();
    let parsed = serde_json::from_slice::<BTreeMap<String, ContainerStats>>(packet.as_bytes())?;

    for (_, container) in parsed {
        let mut tags = BTreeMap::new();
        tags.insert("container_name".into(), container.name.clone());
        tags.insert("container_id".into(), container.id.clone());

        // extracts block io metrics
        for (name, stats) in container.block_io.iter() {
            for item in stats.iter() {
                let mut tags = tags.clone();
                tags.insert("device".into(), format!("{}:{}", item.major, item.minor));
                tags.insert("op".into(), item.op.to_lowercase());

                let counter = Metric {
                    name: format!("aws_ecs_blkio_{}", name),
                    timestamp: Some(container.ts),
                    tags: Some(tags),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: item.value },
                };

                result.push(counter);
            }
        }

        // extracts cpu metrics
        if let Some(cpu) = container.cpu {
            let online_cpus = Metric {
                name: "aws_ecs_cpu_online_cpus".into(),
                timestamp: Some(container.ts),
                tags: Some(tags.clone()),
                kind: MetricKind::Absolute,
                value: MetricValue::Gauge {
                    value: cpu.online_cpus as f64,
                },
            };
            result.push(online_cpus);

            let stats = vec![
                ("system_cpu_usage", cpu.system_cpu_usage),
                ("usage_in_usermode", cpu.cpu_usage.usage_in_usermode),
                ("usage_in_kernelmode", cpu.cpu_usage.usage_in_kernelmode),
                ("total_usage", cpu.cpu_usage.total_usage),
                ("throttling_periods", cpu.throttling_data.periods),
                ("throttled_periods", cpu.throttling_data.throttled_periods),
                ("throttled_time", cpu.throttling_data.throttled_time),
            ];

            for (name, value) in stats.iter() {
                let counter = Metric {
                    name: format!("aws_ecs_cpu_{}", name),
                    timestamp: Some(container.ts),
                    tags: Some(tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: *value },
                };

                result.push(counter);
            }

            for index in 0..cpu.online_cpus {
                let mut tags = tags.clone();
                tags.insert("cpu".into(), index.to_string());

                let counter = Metric {
                    name: "aws_ecs_cpu_percpu_usage".into(),
                    timestamp: Some(container.ts),
                    tags: Some(tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: cpu
                            .cpu_usage
                            .percpu_usage
                            .get(index)
                            .cloned()
                            .unwrap_or_default(),
                    },
                };

                result.push(counter);
            }
        }

        // extracts memory metrics
        if let Some(memory) = container.memory {
            let stats = vec![
                ("usage", memory.usage),
                ("max_usage", memory.max_usage),
                ("limit", memory.limit),
            ];

            for (name, value) in stats.iter() {
                let gauge = Metric {
                    name: format!("aws_ecs_memory_{}", name),
                    timestamp: Some(container.ts),
                    tags: Some(tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: *value },
                };

                result.push(gauge);
            }

            for (name, value) in memory.stats.iter() {
                let gauge = Metric {
                    name: format!("aws_ecs_memory_{}", name),
                    timestamp: Some(container.ts),
                    tags: Some(tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: *value },
                };

                result.push(gauge);
            }
        }

        // extracts network metrics
        for (network, stats) in container.networks.iter() {
            let mut tags = tags.clone();
            tags.insert("network".into(), network.clone());

            for (name, value) in stats.iter() {
                let counter = Metric {
                    name: format!("aws_ecs_network_{}", name),
                    timestamp: Some(container.ts),
                    tags: Some(tags.clone()),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: *value },
                };

                result.push(counter);
            }
        }
    }

    Ok(result)
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
            parse(json).unwrap(),
            vec![
                Metric {
                    name: "aws_ecs_blkio_io_service_bytes_recursive".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "aws_ecs_blkio_io_service_bytes_recursive".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 520192.0 },
                },
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
            parse(json).unwrap(),
            vec![
                Metric {
                    name: "aws_ecs_cpu_online_cpus".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 2.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_system_cpu_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: 2007130000000.0
                    },
                },
                Metric {
                    name: "aws_ecs_cpu_usage_in_usermode".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 510000000.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_usage_in_kernelmode".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 190000000.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_total_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: 2324920942.0
                    },
                },
                Metric {
                    name: "aws_ecs_cpu_throttling_periods".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_throttled_periods".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_throttled_time".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 0.0 },
                },
                Metric {
                    name: "aws_ecs_cpu_percpu_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: 1095931487.0
                    },
                },
                Metric {
                    name: "aws_ecs_cpu_percpu_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter {
                        value: 1228989455.0
                    },
                },
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
                        "active_file": 65536
                    },
                    "limit": 9223372036854771712
                }
            }
        }"##;

        assert_eq!(
            parse(json).unwrap(),
            vec![
                Metric {
                    name: "aws_ecs_memory_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 40120320.0 },
                },
                Metric {
                    name: "aws_ecs_memory_max_usage".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 47177728.0 },
                },
                Metric {
                    name: "aws_ecs_memory_limit".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge {
                        value: 9223372036854771712.0
                    },
                },
                Metric {
                    name: "aws_ecs_memory_active_anon".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 34885632.0 },
                },
                Metric {
                    name: "aws_ecs_memory_active_file".into(),
                    timestamp: Some(ts()),
                    tags: Some(
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
                    kind: MetricKind::Absolute,
                    value: MetricValue::Gauge { value: 65536.0 },
                },
            ],
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
                        "tx_bytes": 2001229
                    }
                }
            }
        }"##;

        assert_eq!(
            parse(json).unwrap(),
            vec![
                Metric {
                    name: "aws_ecs_network_rx_bytes".into(),
                    timestamp: Some(ts()),
                    tags: Some(
                        vec![
                            ("network".into(), "eth1".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 329932716.0 },
                },
                Metric {
                    name: "aws_ecs_network_tx_bytes".into(),
                    timestamp: Some(ts()),
                    tags: Some(
                        vec![
                            ("network".into(), "eth1".into()),
                            (
                                "container_id".into(),
                                "0cf54b87-f0f0-4044-b9d6-20dc54d5c414-4057181352".into()
                            ),
                            ("container_name".into(), "vector2".into())
                        ]
                        .into_iter()
                        .collect()
                    ),
                    kind: MetricKind::Absolute,
                    value: MetricValue::Counter { value: 2001229.0 },
                },
            ],
        );
    }
}
