use sysinfo::Networks;
use vector_lib::{configurable::configurable_component, metric_tags};

use super::{FilterList, HostMetrics, default_all_devices, example_devices};

/// Options for the network metrics collector.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct NetworkConfig {
    /// Lists of device name patterns to include or exclude in gathering
    /// network utilization metrics.
    #[serde(default = "default_all_devices")]
    #[configurable(metadata(docs::examples = "example_devices()"))]
    devices: FilterList,
}

impl HostMetrics {
    pub async fn network_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "network";
        let networks = Networks::new_with_refreshed_list();
        for (interface, data) in &networks {
            if !self
                .config
                .network
                .devices
                .contains_str(Some(interface.as_str()))
            {
                continue;
            }

            let tags = metric_tags!("device" => interface.as_str());
            output.counter(
                "network_receive_bytes_total",
                data.total_received() as f64,
                tags.clone(),
            );
            output.counter(
                "network_receive_errs_total",
                data.total_errors_on_received() as f64,
                tags.clone(),
            );
            output.counter(
                "network_receive_packets_total",
                data.total_packets_received() as f64,
                tags.clone(),
            );
            output.counter(
                "network_transmit_bytes_total",
                data.total_transmitted() as f64,
                tags.clone(),
            );
            output.counter(
                "network_transmit_packets_total",
                data.total_packets_transmitted() as f64,
                tags.clone(),
            );
            // sysinfo doesn't expose drop counters, read from sysfs on linux
            #[cfg(target_os = "linux")]
            if let Some(drops) = read_sysfs_tx_dropped(interface) {
                output.counter(
                    "network_transmit_packets_drop_total",
                    drops as f64,
                    tags.clone(),
                );
            }

            output.counter(
                "network_transmit_errs_total",
                data.total_errors_on_transmitted() as f64,
                tags,
            );
        }
    }
}

#[cfg(target_os = "linux")]
fn read_sysfs_tx_dropped(interface: &str) -> Option<u64> {
    std::fs::read_to_string(format!(
        "/sys/class/net/{interface}/statistics/tx_dropped"
    ))
    .ok()
    .and_then(|s| s.trim().parse().ok())
}

// The Windows CI environment produces zero network metrics, causing
// these tests to always fail.
#[cfg(all(test, not(windows)))]
mod tests {
    use super::{
        super::{
            HostMetrics, HostMetricsConfig, MetricsBuffer,
            tests::{all_counters, assert_filtered_metrics, count_tag},
        },
        NetworkConfig,
    };

    #[tokio::test]
    async fn generates_network_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .network_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
        assert!(!metrics.is_empty());
        assert!(all_counters(&metrics));

        // All metrics are named network_*
        assert!(
            !metrics
                .iter()
                .any(|metric| !metric.name().starts_with("network_"))
        );

        // They should all have a "device" tag
        assert_eq!(count_tag(&metrics, "device"), metrics.len());
    }

    #[tokio::test]
    async fn network_metrics_filters_on_device() {
        assert_filtered_metrics("device", |devices| async move {
            let mut buffer = MetricsBuffer::new(None);
            HostMetrics::new(HostMetricsConfig {
                network: NetworkConfig { devices },
                ..Default::default()
            })
            .network_metrics(&mut buffer)
            .await;
            buffer.metrics
        })
        .await;
    }
}
