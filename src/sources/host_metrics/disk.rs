use crate::internal_events::HostMetricsScrapeDetailError;
use futures::StreamExt;
use heim::units::information::byte;
use vector_lib::configurable::configurable_component;
use vector_lib::metric_tags;

use super::{default_all_devices, example_devices, filter_result, FilterList, HostMetrics};

/// Options for the disk metrics collector.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct DiskConfig {
    /// Lists of device name patterns to include or exclude in gathering
    /// I/O utilization metrics.
    #[configurable(metadata(docs::examples = "example_devices()"))]
    #[serde(default = "default_all_devices")]
    devices: FilterList,
}

impl HostMetrics {
    pub async fn disk_metrics(&self, output: &mut super::MetricsBuffer) {
        match heim::disk::io_counters().await {
            Ok(counters) => {
                for counter in counters
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse disk I/O data.")
                    })
                    .map(|counter| {
                        self.config
                            .disk
                            .devices
                            .contains_path(Some(counter.device_name().as_ref()))
                            .then_some(counter)
                    })
                    .filter_map(|counter| async { counter })
                    .collect::<Vec<_>>()
                    .await
                {
                    let tags = metric_tags! {
                        "device" => counter.device_name().to_string_lossy()
                    };
                    output.name = "disk";
                    output.counter(
                        "disk_read_bytes_total",
                        counter.read_bytes().get::<byte>() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "disk_reads_completed_total",
                        counter.read_count() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "disk_written_bytes_total",
                        counter.write_bytes().get::<byte>() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "disk_writes_completed_total",
                        counter.write_count() as f64,
                        tags,
                    );
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load disk I/O info.",
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
            tests::{all_counters, assert_filtered_metrics, count_name, count_tag},
            HostMetrics, HostMetricsConfig, MetricsBuffer,
        },
        DiskConfig,
    };

    #[tokio::test]
    async fn generates_disk_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .disk_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        // The Windows test runner doesn't generate any disk metrics on the VM.
        #[cfg(not(windows))]
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
        assert_filtered_metrics("device", |devices| async move {
            let mut buffer = MetricsBuffer::new(None);
            HostMetrics::new(HostMetricsConfig {
                disk: DiskConfig { devices },
                ..Default::default()
            })
            .disk_metrics(&mut buffer)
            .await;
            buffer.metrics
        })
        .await;
    }
}
