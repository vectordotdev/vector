use chrono::Utc;
use futures::{stream, StreamExt};
use heim::units::information::byte;
#[cfg(not(target_os = "windows"))]
use heim::units::ratio::ratio;
use serde::{Deserialize, Serialize};
use vector_common::btreemap;

use super::{filter_result, FilterList, HostMetrics};
use crate::event::metric::Metric;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct FilesystemConfig {
    #[serde(default)]
    devices: FilterList,
    #[serde(default)]
    filesystems: FilterList,
    #[serde(default)]
    mountpoints: FilterList,
}

impl HostMetrics {
    pub async fn filesystem_metrics(&self) -> Vec<Metric> {
        match heim::disk::partitions().await {
            Ok(partitions) => {
                partitions
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse partition data.")
                    })
                    // Filter on configured mountpoints
                    .map(|partition| {
                        self.config
                            .filesystem
                            .mountpoints
                            .contains_path(Some(partition.mount_point()))
                            .then(|| partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Filter on configured devices
                    .map(|partition| {
                        self.config
                            .filesystem
                            .devices
                            .contains_path(partition.device().map(|d| d.as_ref()))
                            .then(|| partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Filter on configured filesystems
                    .map(|partition| {
                        self.config
                            .filesystem
                            .filesystems
                            .contains_str(Some(partition.file_system().as_str()))
                            .then(|| partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Load usage from the partition mount point
                    .filter_map(|partition| async {
                        heim::disk::usage(partition.mount_point())
                            .await
                            .map_err(|error| {
                                error!(
                                    message = "Failed to load partition usage data.",
                                    mount_point = ?partition.mount_point(),
                                    %error,
                                    internal_log_rate_secs = 60,
                                )
                            })
                            .map(|usage| (partition, usage))
                            .ok()
                    })
                    .map(|(partition, usage)| {
                        let timestamp = Utc::now();
                        let fs = partition.file_system();
                        let mut tags = btreemap! {
                            "filesystem" => fs.as_str(),
                            "mountpoint" => partition.mount_point().to_string_lossy()
                        };
                        if let Some(device) = partition.device() {
                            tags.insert("device".into(), device.to_string_lossy().into());
                        }
                        stream::iter(
                            vec![
                                self.gauge(
                                    "filesystem_free_bytes",
                                    timestamp,
                                    usage.free().get::<byte>() as f64,
                                    tags.clone(),
                                ),
                                self.gauge(
                                    "filesystem_total_bytes",
                                    timestamp,
                                    usage.total().get::<byte>() as f64,
                                    tags.clone(),
                                ),
                                self.gauge(
                                    "filesystem_used_bytes",
                                    timestamp,
                                    usage.used().get::<byte>() as f64,
                                    tags.clone(),
                                ),
                                #[cfg(not(target_os = "windows"))]
                                self.gauge(
                                    "filesystem_used_ratio",
                                    timestamp,
                                    usage.ratio().get::<ratio>() as f64,
                                    tags,
                                ),
                            ]
                            .into_iter(),
                        )
                    })
                    .flatten()
                    .collect::<Vec<_>>()
                    .await
            }
            Err(error) => {
                error!(message = "Failed to load partitions info.", %error, internal_log_rate_secs = 60);
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            tests::{all_gauges, assert_filtered_metrics, count_name, count_tag},
            HostMetrics, HostMetricsConfig,
        },
        FilesystemConfig,
    };

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn generates_filesystem_metrics() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .filesystem_metrics()
            .await;
        assert!(!metrics.is_empty());
        assert!(metrics.len() % 4 == 0);
        assert!(all_gauges(&metrics));

        // There are exactly three filesystem_* names
        for name in &[
            "filesystem_free_bytes",
            "filesystem_total_bytes",
            "filesystem_used_bytes",
            "filesystem_used_ratio",
        ] {
            assert_eq!(
                count_name(&metrics, name),
                metrics.len() / 4,
                "name={}",
                name
            );
        }

        // They should all have "filesystem" and "mountpoint" tags
        assert_eq!(count_tag(&metrics, "filesystem"), metrics.len());
        assert_eq!(count_tag(&metrics, "mountpoint"), metrics.len());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn generates_filesystem_metrics() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .filesystem_metrics()
            .await;
        assert!(!metrics.is_empty());
        assert!(metrics.len() % 3 == 0);
        assert!(all_gauges(&metrics));

        // There are exactly three filesystem_* names
        for name in &[
            "filesystem_free_bytes",
            "filesystem_total_bytes",
            "filesystem_used_bytes",
        ] {
            assert_eq!(
                count_name(&metrics, name),
                metrics.len() / 3,
                "name={}",
                name
            );
        }

        // They should all have "filesystem" and "mountpoint" tags
        assert_eq!(count_tag(&metrics, "filesystem"), metrics.len());
        assert_eq!(count_tag(&metrics, "mountpoint"), metrics.len());
    }

    #[tokio::test]
    async fn filesystem_metrics_filters_on_device() {
        assert_filtered_metrics("device", |devices| async {
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    devices,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics()
            .await
        })
        .await;
    }

    #[tokio::test]
    async fn filesystem_metrics_filters_on_filesystem() {
        assert_filtered_metrics("filesystem", |filesystems| async {
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    filesystems,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics()
            .await
        })
        .await;
    }

    #[tokio::test]
    async fn filesystem_metrics_filters_on_mountpoint() {
        assert_filtered_metrics("mountpoint", |mountpoints| async {
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    mountpoints,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics()
            .await
        })
        .await;
    }
}
