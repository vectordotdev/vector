use serde::{
    de::{MapAccess, Visitor},
    Deserialize, Deserializer,
};

use crate::event::{Metric, MetricKind, MetricTags, MetricValue};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub proc: Proc,
    pub sys: Sys,
}

impl Stats {
    pub fn metrics(&self, namespace: Option<String>) -> Vec<Metric> {
        let mut result = Vec::new();
        let mut tags = MetricTags::default();
        let now = chrono::Utc::now();
        let namespace = namespace.unwrap_or_else(|| "eventstoredb".to_string());

        tags.replace("id".to_string(), self.proc.id.to_string());

        result.push(
            Metric::new(
                "process_memory_used_bytes",
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: self.proc.mem as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        result.push(
            Metric::new(
                "disk_read_bytes_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: self.proc.disk_io.read_bytes as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        result.push(
            Metric::new(
                "disk_written_bytes_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: self.proc.disk_io.written_bytes as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        result.push(
            Metric::new(
                "disk_read_ops_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: self.proc.disk_io.read_ops as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        result.push(
            Metric::new(
                "disk_write_ops_total",
                MetricKind::Absolute,
                MetricValue::Counter {
                    value: self.proc.disk_io.write_ops as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        result.push(
            Metric::new(
                "memory_free_bytes",
                MetricKind::Absolute,
                MetricValue::Gauge {
                    value: self.sys.free_mem as f64,
                },
            )
            .with_namespace(Some(namespace.clone()))
            .with_tags(Some(tags.clone()))
            .with_timestamp(Some(now)),
        );

        if let Some(drive) = self.sys.drive.as_ref() {
            tags.replace("path".to_string(), drive.path.clone());

            result.push(
                Metric::new(
                    "disk_total_bytes",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: drive.stats.total_bytes as f64,
                    },
                )
                .with_namespace(Some(namespace.clone()))
                .with_tags(Some(tags.clone()))
                .with_timestamp(Some(now)),
            );

            result.push(
                Metric::new(
                    "disk_free_bytes",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: drive.stats.available_bytes as f64,
                    },
                )
                .with_namespace(Some(namespace.clone()))
                .with_tags(Some(tags.clone()))
                .with_timestamp(Some(now)),
            );

            result.push(
                Metric::new(
                    "disk_used_bytes",
                    MetricKind::Absolute,
                    MetricValue::Gauge {
                        value: drive.stats.used_bytes as f64,
                    },
                )
                .with_namespace(Some(namespace))
                .with_tags(Some(tags))
                .with_timestamp(Some(now)),
            );
        }

        result
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Proc {
    pub id: usize,
    pub mem: usize,
    pub cpu: f64,
    pub threads_count: i64,
    pub thrown_exceptions_rate: f64,
    pub disk_io: DiskIo,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DiskIo {
    pub read_bytes: usize,
    pub written_bytes: usize,
    pub read_ops: usize,
    pub write_ops: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Sys {
    pub free_mem: usize,
    pub loadavg: LoadAvg,
    pub drive: Option<Drive>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LoadAvg {
    #[serde(rename = "1m")]
    pub one_m: f64,
    #[serde(rename = "5m")]
    pub five_m: f64,
    #[serde(rename = "15m")]
    pub fifteen_m: f64,
}

#[derive(Debug)]
pub struct Drive {
    pub path: String,
    pub stats: DriveStats,
}

impl<'de> Deserialize<'de> for Drive {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(DriveVisitor)
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DriveStats {
    pub available_bytes: usize,
    pub total_bytes: usize,
    pub usage: String,
    pub used_bytes: usize,
}

struct DriveVisitor;

impl<'de> Visitor<'de> for DriveVisitor {
    type Value = Drive;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "DriveStats object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, <A as MapAccess<'de>>::Error>
    where
        A: MapAccess<'de>,
    {
        if let Some(key) = map.next_key()? {
            return Ok(Drive {
                path: key,
                stats: map.next_value()?,
            });
        }

        Err(serde::de::Error::missing_field("<Drive path>"))
    }
}
