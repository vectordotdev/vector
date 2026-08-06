use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use futures::{StreamExt, stream};
use heim::units::information::byte;
#[cfg(not(windows))]
use heim::units::ratio::ratio;
#[cfg(unix)]
use nix::sys::statvfs::statvfs;
use vector_lib::{configurable::configurable_component, metric_tags};

use super::{
    FilterList, HostMetrics, default_all_devices, example_devices, filter_result, rootfs_root,
};
use crate::internal_events::{HostMetricsScrapeDetailError, HostMetricsScrapeFilesystemError};

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

#[derive(Clone, Debug, Eq, PartialEq)]
struct FilesystemMount {
    source_mountpoint: PathBuf,
    logical_mountpoint: PathBuf,
    lookup_path: PathBuf,
}

fn resolve_filesystem_mount(rootfs_root: Option<&Path>, mount_point: &Path) -> FilesystemMount {
    let source_mountpoint = mount_point.to_path_buf();
    let Some(rootfs_root) = rootfs_root.filter(|root| !root.as_os_str().is_empty()) else {
        return FilesystemMount {
            logical_mountpoint: source_mountpoint.clone(),
            lookup_path: source_mountpoint.clone(),
            source_mountpoint,
        };
    };

    let logical_mountpoint = mount_point
        .strip_prefix(rootfs_root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .map_or_else(
            || {
                if mount_point == rootfs_root {
                    PathBuf::from("/")
                } else {
                    mount_point.to_path_buf()
                }
            },
            |path| Path::new("/").join(path),
        );
    let lookup_path = rootfs_root.join(
        logical_mountpoint
            .strip_prefix("/")
            .unwrap_or(&logical_mountpoint),
    );

    FilesystemMount {
        source_mountpoint,
        logical_mountpoint,
        lookup_path,
    }
}

fn deduplicate_filesystem_mounts<T, F>(
    mut mounts: Vec<(T, FilesystemMount)>,
    tie_breaker: F,
) -> Vec<(T, FilesystemMount)>
where
    F: Fn(&T, &T) -> std::cmp::Ordering,
{
    mounts.sort_by(|left, right| {
        (left.1.source_mountpoint != left.1.lookup_path)
            .cmp(&(right.1.source_mountpoint != right.1.lookup_path))
            .then_with(|| left.1.source_mountpoint.cmp(&right.1.source_mountpoint))
            .then_with(|| tie_breaker(&left.0, &right.0))
    });

    let mut logical_mountpoints = BTreeSet::new();
    mounts
        .into_iter()
        .filter(|(_, mount)| logical_mountpoints.insert(mount.logical_mountpoint.clone()))
        .collect()
}

impl HostMetrics {
    pub async fn filesystem_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "filesystem";
        match heim::disk::partitions().await {
            Ok(partitions) => {
                let rootfs_root = rootfs_root();
                let partitions = partitions
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse partition data.")
                    })
                    .map(|partition| {
                        let mount = resolve_filesystem_mount(
                            rootfs_root.as_deref(),
                            partition.mount_point(),
                        );
                        (partition, mount)
                    })
                    .collect::<Vec<_>>()
                    .await;
                let partitions = deduplicate_filesystem_mounts(partitions, |left, right| {
                    left.device().cmp(&right.device()).then_with(|| {
                        left.file_system()
                            .as_str()
                            .cmp(right.file_system().as_str())
                    })
                })
                .into_iter()
                // Filter on configured logical mountpoints.
                .filter(|(_, mount)| {
                    self.config
                        .filesystem
                        .mountpoints
                        .contains_path(Some(&mount.logical_mountpoint))
                })
                // Filter on configured devices.
                .filter(|(partition, _)| {
                    self.config
                        .filesystem
                        .devices
                        .contains_path(partition.device().map(|device| device.as_ref()))
                })
                // Filter on configured filesystems.
                .filter(|(partition, _)| {
                    self.config
                        .filesystem
                        .filesystems
                        .contains_str(Some(partition.file_system().as_str()))
                })
                .collect::<Vec<_>>();

                for (partition, mount, usage) in stream::iter(partitions)
                    // Load usage from the partition mount point.
                    .filter_map(|(partition, mount)| async {
                        heim::disk::usage(&mount.lookup_path)
                            .await
                            .map_err(|error| {
                                emit!(HostMetricsScrapeFilesystemError {
                                    message: "Failed to load partitions info.",
                                    mount_point: mount
                                        .logical_mountpoint
                                        .to_string_lossy()
                                        .to_string(),
                                    resolved_mount_point: mount
                                        .lookup_path
                                        .to_string_lossy()
                                        .to_string(),
                                    error,
                                })
                            })
                            .map(|usage| (partition, mount, usage))
                            .ok()
                    })
                    .collect::<Vec<_>>()
                    .await
                {
                    let fs = partition.file_system();
                    let mut tags = metric_tags! {
                        "filesystem" => fs.as_str(),
                        "mountpoint" => mount.logical_mountpoint.to_string_lossy()
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
                        tags.clone(),
                    );

                    // inode metrics via a second statvfs call - heim's Usage wraps
                    // libc::statvfs internally but doesn't expose inode fields
                    // (f_files, f_ffree). the kernel caches statvfs for local
                    // filesystems so the overhead is negligible, but network mounts
                    // may pay a small extra cost.
                    #[cfg(unix)]
                    if let Ok(stat) = statvfs(&mount.lookup_path) {
                        let inodes_total = stat.files() as f64;
                        let inodes_free = stat.files_free() as f64;
                        let inodes_used = (inodes_total - inodes_free).max(0.0);
                        let inodes_used_ratio = if inodes_total > 0.0 {
                            inodes_used / inodes_total
                        } else {
                            0.0
                        };

                        output.gauge("filesystem_inodes_total", inodes_total, tags.clone());
                        output.gauge("filesystem_inodes_free", inodes_free, tags.clone());
                        output.gauge("filesystem_inodes_used", inodes_used, tags.clone());
                        output.gauge("filesystem_inodes_used_ratio", inodes_used_ratio, tags);
                    }
                    #[cfg(windows)]
                    drop(tags);
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
            HostMetrics, HostMetricsConfig, MetricsBuffer,
            tests::{all_gauges, assert_filtered_metrics, count_name, count_tag},
        },
        FilesystemConfig, FilesystemMount, deduplicate_filesystem_mounts, resolve_filesystem_mount,
    };
    use std::path::Path;

    fn assert_mount(
        rootfs_root: Option<&Path>,
        source_mountpoint: &str,
        logical_mountpoint: &str,
        lookup_path: &str,
    ) {
        assert_eq!(
            resolve_filesystem_mount(rootfs_root, Path::new(source_mountpoint)),
            FilesystemMount {
                source_mountpoint: source_mountpoint.into(),
                logical_mountpoint: logical_mountpoint.into(),
                lookup_path: lookup_path.into(),
            }
        );
    }

    #[test]
    fn resolves_filesystem_mounts() {
        assert_mount(None, "/srv", "/srv", "/srv");
        assert_mount(Some(Path::new("")), "/srv", "/srv", "/srv");
        assert_mount(Some(Path::new("/")), "/", "/", "/");
        assert_mount(Some(Path::new("/")), "/srv", "/srv", "/srv");
        assert_mount(Some(Path::new("/host")), "/", "/", "/host");
        assert_mount(Some(Path::new("/host")), "/host", "/", "/host");
        assert_mount(Some(Path::new("/host/")), "/host/", "/", "/host/");
        assert_mount(Some(Path::new("/host")), "/host/srv", "/srv", "/host/srv");
        assert_mount(
            Some(Path::new("/host")),
            "/host/srv/vector",
            "/srv/vector",
            "/host/srv/vector",
        );
        assert_mount(Some(Path::new("/host")), "/srv", "/srv", "/host/srv");
        assert_mount(
            Some(Path::new("/host")),
            "/hosted",
            "/hosted",
            "/host/hosted",
        );
    }

    #[test]
    fn deduplicates_equivalent_logical_mounts() {
        let mounts = deduplicate_filesystem_mounts(
            vec![
                (
                    "container-root",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/")),
                ),
                (
                    "host-root",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/host")),
                ),
                (
                    "host-srv",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/host/srv")),
                ),
                (
                    "container-srv",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/srv")),
                ),
            ],
            Ord::cmp,
        );

        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].0, "host-root");
        assert_eq!(mounts[0].1.logical_mountpoint, Path::new("/"));
        assert_eq!(mounts[0].1.source_mountpoint, Path::new("/host"));
        assert_eq!(mounts[0].1.lookup_path, Path::new("/host"));
        assert_eq!(mounts[1].0, "host-srv");
        assert_eq!(mounts[1].1.logical_mountpoint, Path::new("/srv"));
        assert_eq!(mounts[1].1.source_mountpoint, Path::new("/host/srv"));
        assert_eq!(mounts[1].1.lookup_path, Path::new("/host/srv"));

        let stacked_mounts = deduplicate_filesystem_mounts(
            vec![
                (
                    "z-device",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/host/tmp")),
                ),
                (
                    "a-device",
                    resolve_filesystem_mount(Some(Path::new("/host")), Path::new("/host/tmp")),
                ),
            ],
            Ord::cmp,
        );
        assert_eq!(stacked_mounts.len(), 1);
        assert_eq!(stacked_mounts[0].0, "a-device");
    }

    #[cfg(not(windows))]
    #[tokio::test]
    async fn generates_filesystem_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .filesystem_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
        assert!(!metrics.is_empty());
        assert!(all_gauges(&metrics));

        // Base metrics (these are always present)
        let base_metrics = [
            "filesystem_free_bytes",
            "filesystem_total_bytes",
            "filesystem_used_bytes",
            "filesystem_used_ratio",
        ];

        // Each filesystem should have all 4 base metrics
        let num_filesystems = count_name(&metrics, "filesystem_free_bytes");
        assert!(num_filesystems > 0);
        for name in &base_metrics {
            assert_eq!(count_name(&metrics, name), num_filesystems, "name={name}");
        }

        // Inode metrics are present for filesystems that support statvfs
        // (some virtual filesystems like /proc, /sys might not)
        let inode_metrics = [
            "filesystem_inodes_total",
            "filesystem_inodes_free",
            "filesystem_inodes_used",
            "filesystem_inodes_used_ratio",
        ];
        let num_inode_total = count_name(&metrics, "filesystem_inodes_total");
        assert!(
            num_inode_total > 0,
            "Expected at least one filesystem to report inode metrics"
        );

        // For filesystems that report inodes, all 4 inode metrics should be present
        for name in &inode_metrics {
            assert_eq!(count_name(&metrics, name), num_inode_total, "name={name}");
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
