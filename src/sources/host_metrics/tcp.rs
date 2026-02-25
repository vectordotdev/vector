use std::{collections::HashMap, path::Path};

use procfs::net::{TcpNetEntry, TcpState};
use snafu::{ResultExt, Snafu};
use vector_lib::event::MetricTags;

use super::HostMetrics;
use crate::sources::host_metrics::HostMetricsScrapeDetailError;

const PROC_IPV6_FILE: &str = "/proc/net/if_inet6";
const TCP_CONNS_TOTAL: &str = "tcp_connections_total";
const TCP_TX_QUEUED_BYTES_TOTAL: &str = "tcp_tx_queued_bytes_total";
const TCP_RX_QUEUED_BYTES_TOTAL: &str = "tcp_rx_queued_bytes_total";
const STATE: &str = "state";

impl HostMetrics {
    pub async fn tcp_metrics(&self, output: &mut super::MetricsBuffer) {
        match build_tcp_stats() {
            Ok(stats) => {
                output.name = "tcp";
                for (state, count) in stats.conn_states {
                    let tags = metric_tags! {
                        STATE => state
                    };
                    output.gauge(TCP_CONNS_TOTAL, count, tags);
                }

                output.gauge(
                    TCP_TX_QUEUED_BYTES_TOTAL,
                    stats.tx_queued_bytes,
                    MetricTags::default(),
                );
                output.gauge(
                    TCP_RX_QUEUED_BYTES_TOTAL,
                    stats.rx_queued_bytes,
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

#[derive(Debug, Snafu)]
enum TcpError {
    #[snafu(display("Could not read TCP socket table: {}", source))]
    ReadTcpTable { source: procfs::ProcError },
    #[snafu(display("Could not read TCP6 socket table: {}", source))]
    ReadTcp6Table { source: procfs::ProcError },
}

#[derive(Debug, Default)]
struct TcpStats {
    conn_states: HashMap<String, f64>,
    rx_queued_bytes: f64,
    tx_queued_bytes: f64,
}

fn tcp_state_to_string(state: TcpState) -> &'static str {
    match state {
        TcpState::Established => "established",
        TcpState::SynSent => "syn_sent",
        TcpState::SynRecv => "syn_recv",
        TcpState::FinWait1 => "fin_wait1",
        TcpState::FinWait2 => "fin_wait2",
        TcpState::TimeWait => "time_wait",
        TcpState::Close => "close",
        TcpState::CloseWait => "close_wait",
        TcpState::LastAck => "last_ack",
        TcpState::Listen => "listen",
        TcpState::Closing => "closing",
        TcpState::NewSynRecv => "new_syn_recv",
    }
}

fn parse_tcp_entries(entries: Vec<TcpNetEntry>, tcp_stats: &mut TcpStats) {
    for entry in entries {
        let state_str = tcp_state_to_string(entry.state);
        *tcp_stats.conn_states.entry(state_str).or_insert(0.0) += 1.0;
        tcp_stats.tx_queued_bytes += f64::from(entry.tx_queue);
        tcp_stats.rx_queued_bytes += f64::from(entry.rx_queue);
    }
}

fn build_tcp_stats() -> Result<TcpStats, TcpError> {
    let mut tcp_stats = TcpStats::default();

    // Read IPv4 TCP sockets
    let tcp_entries = procfs::net::tcp().context(ReadTcpTableSnafu)?;
    parse_tcp_entries(tcp_entries, &mut tcp_stats);

    // Read IPv6 TCP sockets if IPv6 is enabled
    if is_ipv6_enabled() {
        let tcp6_entries = procfs::net::tcp6().context(ReadTcp6TableSnafu)?;
        parse_tcp_entries(tcp6_entries, &mut tcp_stats);
    }

    Ok(tcp_stats)
}

fn is_ipv6_enabled() -> bool {
    Path::new(PROC_IPV6_FILE).exists()
}

#[cfg(test)]
mod tests {
    use procfs::net::TcpState;

    use super::{
        tcp_state_to_string, STATE, TCP_CONNS_TOTAL, TCP_RX_QUEUED_BYTES_TOTAL,
        TCP_TX_QUEUED_BYTES_TOTAL,
    };
    use crate::sources::host_metrics::{HostMetrics, HostMetricsConfig, MetricsBuffer};

    #[test]
    fn tcp_state_to_string_handles_all_variants() {
        // Verify all 12 TCP states map correctly
        assert_eq!(tcp_state_to_string(TcpState::Established), "established");
        assert_eq!(tcp_state_to_string(TcpState::SynSent), "syn_sent");
        assert_eq!(tcp_state_to_string(TcpState::SynRecv), "syn_recv");
        assert_eq!(tcp_state_to_string(TcpState::FinWait1), "fin_wait1");
        assert_eq!(tcp_state_to_string(TcpState::FinWait2), "fin_wait2");
        assert_eq!(tcp_state_to_string(TcpState::TimeWait), "time_wait");
        assert_eq!(tcp_state_to_string(TcpState::Close), "close");
        assert_eq!(tcp_state_to_string(TcpState::CloseWait), "close_wait");
        assert_eq!(tcp_state_to_string(TcpState::LastAck), "last_ack");
        assert_eq!(tcp_state_to_string(TcpState::Listen), "listen");
        assert_eq!(tcp_state_to_string(TcpState::Closing), "closing");
        assert_eq!(tcp_state_to_string(TcpState::NewSynRecv), "new_syn_recv");
    }

    #[tokio::test]
    async fn generates_tcp_metrics() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .tcp_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert!(!metrics.is_empty());

        let mut n_tx_queued_bytes_metric = 0;
        let mut n_rx_queued_bytes_metric = 0;
        let mut n_conn_total_metrics = 0;

        for metric in metrics {
            if metric.name() == TCP_CONNS_TOTAL {
                n_conn_total_metrics += 1;
                let tags = metric.tags();
                assert!(
                    tags.is_some(),
                    "Metric tcp_connections_total must have a tag"
                );
                let tags = tags.unwrap();
                assert!(
                    tags.contains_key(STATE),
                    "Metric tcp_connections_total must have a state tag"
                );
            } else if metric.name() == TCP_TX_QUEUED_BYTES_TOTAL {
                n_tx_queued_bytes_metric += 1;
            } else if metric.name() == TCP_RX_QUEUED_BYTES_TOTAL {
                n_rx_queued_bytes_metric += 1;
            } else {
                panic!("unrecognized metric name: {}", metric.name());
            }
        }

        assert!(
            n_conn_total_metrics > 0,
            "Expected at least one tcp_connections_total metric"
        );
        assert_eq!(n_tx_queued_bytes_metric, 1);
        assert_eq!(n_rx_queued_bytes_metric, 1);
    }
}
