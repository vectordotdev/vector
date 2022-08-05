use std::collections::BTreeMap;

use futures::StreamExt;
#[cfg(target_os = "linux")]
use heim::net::os::linux::IoCountersExt;
#[cfg(target_os = "windows")]
use heim::net::os::windows::IoCountersExt;
use heim::units::information::byte;
use vector_config::configurable_component;

use super::{filter_result, FilterList, HostMetrics};

/// Options for the “network” metrics collector.
#[configurable_component]
#[derive(Clone, Debug, Default)]
pub struct NetworkConfig {
    /// Lists of device name patterns to include or exclude.
    #[serde(default)]
    devices: FilterList,
}

impl HostMetrics {
    pub async fn network_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "network";
        match heim::net::io_counters().await {
            Ok(counters) => {
                for counter in counters
                    .filter_map(|result| {
                        filter_result(result, "Failed to load/parse network data.")
                    })
                    // The following pair should be possible to do in one
                    // .filter_map, but it results in a strange "one type is
                    // more general than the other" error.
                    .map(|counter| {
                        self.config
                            .network
                            .devices
                            .contains_str(Some(counter.interface()))
                            .then(|| counter)
                    })
                    .filter_map(|counter| async { counter })
                    .collect::<Vec<_>>()
                    .await
                {
                    let interface = counter.interface();
                    output.counter(
                        "network_receive_bytes_total",
                        counter.bytes_recv().get::<byte>() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    output.counter(
                        "network_receive_errs_total",
                        counter.errors_recv() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    output.counter(
                        "network_receive_packets_total",
                        counter.packets_recv() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    output.counter(
                        "network_transmit_bytes_total",
                        counter.bytes_sent().get::<byte>() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    output.counter(
                        "network_transmit_errs_total",
                        counter.errors_sent() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    #[cfg(any(target_os = "linux", target_os = "windows"))]
                    output.counter(
                        "network_transmit_packets_drop_total",
                        counter.drop_sent() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                    #[cfg(any(target_os = "linux", target_os = "windows"))]
                    output.counter(
                        "network_transmit_packets_total",
                        counter.packets_sent() as f64,
                        BTreeMap::from([(String::from("device"), interface.to_string())]),
                    );
                }
            }
            Err(error) => {
                error!(message = "Failed to load network I/O counters.", %error, internal_log_rate_secs = 60);
            }
        }
    }
}

// The Windows CI environment produces zero network metrics, causing
// these tests to always fail.
#[cfg(all(test, not(target_os = "windows")))]
mod tests {
    use super::{
        super::{
            tests::{all_counters, assert_filtered_metrics, count_tag},
            HostMetrics, HostMetricsConfig, MetricsBuffer,
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
        assert!(!metrics
            .iter()
            .any(|metric| !metric.name().starts_with("network_")));

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
