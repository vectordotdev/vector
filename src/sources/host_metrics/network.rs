use futures::StreamExt;
#[cfg(target_os = "linux")]
use heim::net::os::linux::IoCountersExt;
#[cfg(windows)]
use heim::net::os::windows::IoCountersExt;
use heim::units::information::byte;
use vector_lib::configurable::configurable_component;
use vector_lib::metric_tags;

use crate::internal_events::HostMetricsScrapeDetailError;

#[cfg(target_os = "linux")]
use super::netlink_tcp;
use super::{
    default_all_devices, example_devices, filter_result, FilterList, HostMetrics, MetricTags,
};

const NETWORK_TCP_CONNS_TOTAL: &str = "network_tcp_connections_total";
const NETWORK_TCP_TX_QUEUED_BYTES_TOTAL: &str = "network_tcp_tx_queued_bytes_total";
const NETWORK_TCP_RX_QUEUED_BYTES_TOTAL: &str = "network_tcp_rx_queued_bytes_total";
const TCP_CONN_STATE: &str = "state";

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
                            .then_some(counter)
                    })
                    .filter_map(|counter| async { counter })
                    .collect::<Vec<_>>()
                    .await
                {
                    let interface = counter.interface();
                    let tags = metric_tags!("device" => interface);
                    output.counter(
                        "network_receive_bytes_total",
                        counter.bytes_recv().get::<byte>() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "network_receive_errs_total",
                        counter.errors_recv() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "network_receive_packets_total",
                        counter.packets_recv() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "network_transmit_bytes_total",
                        counter.bytes_sent().get::<byte>() as f64,
                        tags.clone(),
                    );
                    #[cfg(any(target_os = "linux", windows))]
                    output.counter(
                        "network_transmit_packets_drop_total",
                        counter.drop_sent() as f64,
                        tags.clone(),
                    );
                    #[cfg(any(target_os = "linux", windows))]
                    output.counter(
                        "network_transmit_packets_total",
                        counter.packets_sent() as f64,
                        tags.clone(),
                    );
                    output.counter(
                        "network_transmit_errs_total",
                        counter.errors_sent() as f64,
                        tags,
                    );
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load network I/O counters.",
                    error,
                });
            }
        }

        #[cfg(target_os = "linux")]
        match netlink_tcp::build_tcp_stats().await {
            Ok(stats) => {
                output.name = "tcp";
                for (state, count) in stats.conn_states() {
                    let state_str: String = state.into();
                    let tags = metric_tags! {
                        TCP_CONN_STATE => state_str
                    };
                    output.gauge(NETWORK_TCP_CONNS_TOTAL, (*count).into(), tags);
                }

                output.gauge(
                    NETWORK_TCP_TX_QUEUED_BYTES_TOTAL,
                    stats.tx_queued_bytes().into(),
                    MetricTags::default(),
                );
                output.gauge(
                    NETWORK_TCP_RX_QUEUED_BYTES_TOTAL,
                    stats.rx_queued_bytes().into(),
                    MetricTags::default(),
                );
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load tcp connection info.",
                    error,
                });
            }
        }
    }
}

// The Windows CI environment produces zero network metrics, causing
// these tests to always fail.
#[cfg(all(test, not(windows)))]
mod tests {
    use super::{
        super::{
            tests::assert_filtered_metrics, HostMetrics, HostMetricsConfig, MetricValue,
            MetricsBuffer,
        },
        NetworkConfig, NETWORK_TCP_CONNS_TOTAL, NETWORK_TCP_RX_QUEUED_BYTES_TOTAL,
        NETWORK_TCP_TX_QUEUED_BYTES_TOTAL, TCP_CONN_STATE,
    };
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn generates_network_metrics() {
        let _listener = TcpListener::bind("127.0.0.1:0").await.unwrap();

        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .network_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;
        assert!(!metrics.is_empty());

        // All metrics are named network_*
        assert!(!metrics
            .iter()
            .any(|metric| !metric.name().starts_with("network_")));

        // TCP related metrics is a gauge; everything else is a counter
        metrics.iter().for_each(|metric| {
            if metric.name().contains("tcp") {
                assert!(matches!(metric.value(), &MetricValue::Gauge { .. }))
            } else {
                assert!(matches!(metric.value(), &MetricValue::Counter { .. }))
            }
        });

        // All non TCP related metrics should have a "device" tag
        metrics
            .iter()
            .filter(|metric| {
                // skip all TCP related metrics
                if metric.name().contains("tcp") {
                    return false;
                }
                true
            })
            .for_each(|metric| {
                assert!(metric
                    .tags()
                    .expect("Metric is missing the 'device' tag")
                    .contains_key("device"));
            });

        // Assert that the metrics buffer contains the TCP related metrics
        // and the network_tcp_connections_total has the "state" tag.
        #[cfg(target_os = "linux")]
        {
            let mut n_tx_queued_bytes_metric = 0;
            let mut n_rx_queued_bytes_metric = 0;

            metrics.iter().for_each(|metric| {
                if metric.name() == NETWORK_TCP_CONNS_TOTAL {
                    let tags = metric.tags().unwrap();
                    assert!(
                        tags.contains_key(TCP_CONN_STATE),
                        "Metric tcp_connections_total must have a mode tag"
                    );
                } else if metric.name() == NETWORK_TCP_TX_QUEUED_BYTES_TOTAL {
                    n_tx_queued_bytes_metric += 1;
                } else if metric.name() == NETWORK_TCP_RX_QUEUED_BYTES_TOTAL {
                    n_rx_queued_bytes_metric += 1;
                } else {
                    return;
                }
            });
            assert_eq!(n_tx_queued_bytes_metric, 1);
            assert_eq!(n_rx_queued_bytes_metric, 1);
        }
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
