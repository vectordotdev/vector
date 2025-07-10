use futures::StreamExt;
use heim::units::information::byte;
#[cfg(not(windows))]
use heim::units::ratio::ratio;
use vector_lib::configurable::configurable_component;
use vector_lib::metric_tags;

use crate::internal_events::{HostMetricsScrapeDetailError, HostMetricsScrapeFilesystemError};

use super::{default_all_devices, example_devices, filter_result, FilterList, HostMetrics};

/// Options for the filesystem metrics collector.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct FilesystemConfig {
    /// Lists of device name patterns to include or exclude in gathering
    /// usage metrics.
    #[serde(default = "default_all_devices")]
    #[configurable(metadata(docs::examples = "example_devices()"))]
    devices: FilterList,

    /// Lists of filesystem name patterns to include or exclude in gathering
    /// usage metrics.
    #[serde(default = "default_all_devices")]
    #[configurable(metadata(docs::examples = "example_filesystems()"))]
    filesystems: FilterList,

    /// Lists of mount point path patterns to include or exclude in gathering
    /// usage metrics.
    #[serde(default = "default_all_devices")]
    #[configurable(metadata(docs::examples = "example_mountpoints()"))]
    mountpoints: FilterList,
}

fn example_filesystems() -> FilterList {
    FilterList {
        includes: Some(vec!["ntfs".try_into().unwrap()]),
        excludes: Some(vec!["ext*".try_into().unwrap()]),
    }
}

fn example_mountpoints() -> FilterList {
    FilterList {
        includes: Some(vec!["/home".try_into().unwrap()]),
        excludes: Some(vec!["/raid*".try_into().unwrap()]),
    }
}

impl HostMetrics {
    pub async fn filesystem_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "filesystem";
        match heim::disk::partitions().await {
            Ok(partitions) => {
                for (partition, usage) in partitions
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse partition data.")
                    })
                    // Filter on configured mountpoints
                    .map(|partition| {
                        self.config
                            .filesystem
                            .mountpoints
                            .contains_path(Some(partition.mount_point()))
                            .then_some(partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Filter on configured devices
                    .map(|partition| {
                        self.config
                            .filesystem
                            .devices
                            .contains_path(partition.device().map(|d| d.as_ref()))
                            .then_some(partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Filter on configured filesystems
                    .map(|partition| {
                        self.config
                            .filesystem
                            .filesystems
                            .contains_str(Some(partition.file_system().as_str()))
                            .then_some(partition)
                    })
                    .filter_map(|partition| async { partition })
                    // Load usage from the partition mount point
                    .filter_map(|partition| async {
                        heim::disk::usage(partition.mount_point())
                            .await
                            .map_err(|error| {
                                emit!(HostMetricsScrapeFilesystemError {
                                    message: "Failed to load partitions info.",
                                    mount_point: partition
                                        .mount_point()
                                        .to_str()
                                        .unwrap_or("unknown")
                                        .to_string(),
                                    error,
                                })
                            })
                            .map(|usage| (partition, usage))
                            .ok()
                    })
                    .collect::<Vec<_>>()
                    .await
                {
                    let fs = partition.file_system();
                    let mut tags = metric_tags! {
                        "filesystem" => fs.as_str(),
                        "mountpoint" => partition.mount_point().to_string_lossy()
                    };
                    if let Some(device) = partition.device() {
                        tags.replace("device".into(), device.to_string_lossy().to_string());
                    }
                    output.gauge(
                        "filesystem_free_bytes",
                        usage.free().get::<byte>() as f64,
                        tags.clone(),
                    );
                    output.gauge(
                        "filesystem_total_bytes",
                        usage.total().get::<byte>() as f64,
                        tags.clone(),
                    );
                    output.gauge(
                        "filesystem_used_bytes",
                        usage.used().get::<byte>() as f64,
                        tags.clone(),
                    );
                    #[cfg(not(windows))]
                    output.gauge(
                        "filesystem_used_ratio",
                        usage.ratio().get::<ratio>() as f64,
                        tags,
                    );
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load partitions info.",
                    error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            tests::{all_gauges, assert_filtered_metrics, count_name, count_tag},
            HostMetrics, HostMetricsConfig, MetricsBuffer,
        },
        FilesystemConfig,
    };

    #[cfg(not(windows))]
    #[tokio::test]
    async fn generates_filesystem_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .filesystem_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
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

    #[cfg(windows)]
    #[tokio::test]
    async fn generates_filesystem_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .filesystem_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
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
        assert_filtered_metrics("device", |devices| async move {
            let mut buffer = MetricsBuffer::new(None);
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    devices,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics(&mut buffer)
            .await;
            buffer.metrics
        })
        .await;
    }

    #[tokio::test]
    async fn filesystem_metrics_filters_on_filesystem() {
        assert_filtered_metrics("filesystem", |filesystems| async move {
            let mut buffer = MetricsBuffer::new(None);
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    filesystems,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics(&mut buffer)
            .await;
            buffer.metrics
        })
        .await;
    }

    #[tokio::test]
    async fn filesystem_metrics_filters_on_mountpoint() {
        assert_filtered_metrics("mountpoint", |mountpoints| async move {
            let mut buffer = MetricsBuffer::new(None);
            HostMetrics::new(HostMetricsConfig {
                filesystem: FilesystemConfig {
                    mountpoints,
                    ..Default::default()
                },
                ..Default::default()
            })
            .filesystem_metrics(&mut buffer)
            .await;
            buffer.metrics
        })
        .await;
    }
}
