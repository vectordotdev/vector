use chrono::Utc;
use futures::{stream, StreamExt};
use heim::units::information::byte;
use serde::{Deserialize, Serialize};
use vector_common::btreemap;

use super::{filter_result, FilterList, HostMetrics};
use crate::event::metric::Metric;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct DiskConfig {
    #[serde(default)]
    devices: FilterList,
}

impl HostMetrics {
    pub async fn disk_metrics(&self) -> Vec<Metric> {
        match heim::disk::io_counters().await {
            Ok(counters) => {
                counters
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse disk I/O data.")
                    })
                    .map(|counter| {
                        self.config
                            .disk
                            .devices
                            .contains_path(Some(counter.device_name().as_ref()))
                            .then(|| counter)
                    })
                    .filter_map(|counter| async { counter })
                    .map(|counter| {
                        let timestamp = Utc::now();
                        let tags = btreemap! {
                            "device" => counter.device_name().to_string_lossy()
                        };
                        stream::iter(
                            vec![
                                self.counter(
                                    "disk_read_bytes_total",
                                    timestamp,
                                    counter.read_bytes().get::<byte>() as f64,
                                    tags.clone(),
                                ),
                                self.counter(
                                    "disk_reads_completed_total",
                                    timestamp,
                                    counter.read_count() as f64,
                                    tags.clone(),
                                ),
                                self.counter(
                                    "disk_written_bytes_total",
                                    timestamp,
                                    counter.write_bytes().get::<byte>() as f64,
                                    tags.clone(),
                                ),
                                self.counter(
                                    "disk_writes_completed_total",
                                    timestamp,
                                    counter.write_count() as f64,
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
                error!(message = "Failed to load disk I/O info.", %error, internal_log_rate_secs = 60);
                vec![]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        super::{
            tests::{all_counters, assert_filtered_metrics, count_name, count_tag},
            HostMetrics, HostMetricsConfig,
        },
        DiskConfig,
    };

    #[tokio::test]
    async fn generates_disk_metrics() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .disk_metrics()
            .await;
        // The Windows test runner doesn't generate any disk metrics on the VM.
        #[cfg(not(target_os = "windows"))]
        assert!(!metrics.is_empty());
        assert!(metrics.len() % 4 == 0);
        assert!(all_counters(&metrics));

        // There are exactly four disk_* names
        for name in &[
            "disk_read_bytes_total",
            "disk_reads_completed_total",
            "disk_written_bytes_total",
            "disk_writes_completed_total",
        ] {
            assert_eq!(
                count_name(&metrics, name),
                metrics.len() / 4,
                "name={}",
                name
            );
        }

        // They should all have a "device" tag
        assert_eq!(count_tag(&metrics, "device"), metrics.len());
    }

    #[tokio::test]
    async fn filters_disk_metrics_on_device() {
        assert_filtered_metrics("device", |devices| async {
            HostMetrics::new(HostMetricsConfig {
                disk: DiskConfig { devices },
                ..Default::default()
            })
            .disk_metrics()
            .await
        })
        .await;
    }
}
