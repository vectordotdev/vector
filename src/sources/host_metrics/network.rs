use chrono::Utc;
use futures::{stream, StreamExt};
#[cfg(target_os = "linux")]
use heim::net::os::linux::IoCountersExt;
#[cfg(target_os = "windows")]
use heim::net::os::windows::IoCountersExt;
use heim::units::information::byte;
use serde::{Deserialize, Serialize};
use vector_common::btreemap;

use super::{filter_result, FilterList, HostMetrics};
use crate::event::metric::Metric;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct NetworkConfig {
    #[serde(default)]
    devices: FilterList,
}

impl HostMetrics {
    pub async fn network_metrics(&self) -> Vec<Metric> {
        match heim::net::io_counters().await {
            Ok(counters) => {
                counters
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
                    .map(|counter| {
                        let timestamp = Utc::now();
                        let interface = counter.interface();
                        stream::iter(
                            vec![
                                self.counter(
                                    "network_receive_bytes_total",
                                    timestamp,
                                    counter.bytes_recv().get::<byte>() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                self.counter(
                                    "network_receive_errs_total",
                                    timestamp,
                                    counter.errors_recv() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                self.counter(
                                    "network_receive_packets_total",
                                    timestamp,
                                    counter.packets_recv() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                self.counter(
                                    "network_transmit_bytes_total",
                                    timestamp,
                                    counter.bytes_sent().get::<byte>() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                self.counter(
                                    "network_transmit_errs_total",
                                    timestamp,
                                    counter.errors_sent() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                #[cfg(any(target_os = "linux", target_os = "windows"))]
                                self.counter(
                                    "network_transmit_packets_drop_total",
                                    timestamp,
                                    counter.drop_sent() as f64,
                                    btreemap! { "device" => interface },
                                ),
                                #[cfg(any(target_os = "linux", target_os = "windows"))]
                                self.counter(
                                    "network_transmit_packets_total",
                                    timestamp,
                                    counter.packets_sent() as f64,
                                    btreemap! { "device" => interface },
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
                error!(message = "Failed to load network I/O counters.", %error, internal_log_rate_secs = 60);
                vec![]
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
            HostMetrics, HostMetricsConfig,
        },
        NetworkConfig,
    };

    #[tokio::test]
    async fn generates_network_metrics() {
        let metrics = HostMetrics::new(HostMetricsConfig::default())
            .network_metrics()
            .await;
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
        assert_filtered_metrics("device", |devices| async {
            HostMetrics::new(HostMetricsConfig {
                network: NetworkConfig { devices },
                ..Default::default()
            })
            .network_metrics()
            .await
        })
        .await;
    }
}
