use async_graphql::Object;

use crate::{
    event::{Metric, MetricValue},
    sources::host_metrics,
};

pub struct MemoryMetrics(Vec<Metric>);

#[Object]
/// Host memory metrics
impl MemoryMetrics {
    /// Total bytes
    async fn total_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_total_bytes")
    }

    /// Free bytes
    async fn free_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_free_bytes")
    }

    /// Available bytes
    async fn available_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_available_bytes")
    }

    /// Active bytes (Linux/macOS only)
    async fn active_bytes(&self) -> Option<f64> {
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            Some(filter_host_metric(&self.0, "memory_active_bytes"))
        } else {
            None
        }
    }

    /// Buffers bytes (Linux only)
    async fn buffers_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_buffers_bytes"))
        } else {
            None
        }
    }

    /// Cached bytes (Linux only)
    async fn cached_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_cached_bytes"))
        } else {
            None
        }
    }

    /// Shared bytes (Linux only)
    async fn shared_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_shared_bytes"))
        } else {
            None
        }
    }

    /// Used bytes (Linux only)
    async fn used_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_used_bytes"))
        } else {
            None
        }
    }

    /// Inactive bytes (macOS only)
    async fn inactive_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "macos") {
            Some(filter_host_metric(&self.0, "memory_inactive_bytes"))
        } else {
            None
        }
    }

    /// Wired bytes (macOS only)
    async fn wired_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "macos") {
            Some(filter_host_metric(&self.0, "memory_wired_bytes"))
        } else {
            None
        }
    }
}

pub struct SwapMetrics(Vec<Metric>);

#[Object]
impl SwapMetrics {
    /// Swap free bytes
    async fn free_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_swap_free_bytes")
    }

    /// Swap total bytes
    async fn total_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_swap_total_bytes")
    }

    /// Swap used bytes
    async fn used_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "memory_swap_used_bytes")
    }

    /// Swapped in bytes total (not available on Windows)
    async fn swapped_in_bytes_total(&self) -> Option<f64> {
        if cfg!(not(windows)) {
            Some(filter_host_metric(&self.0, "memory_swapped_in_bytes_total"))
        } else {
            None
        }
    }

    /// Swapped out bytes total (not available on Windows)
    async fn swapped_out_bytes_total(&self) -> Option<f64> {
        if cfg!(not(windows)) {
            Some(filter_host_metric(
                &self.0,
                "memory_swapped_out_bytes_total",
            ))
        } else {
            None
        }
    }
}

pub struct CpuMetrics(Vec<Metric>);

#[Object]
impl CpuMetrics {
    /// CPU seconds total
    async fn cpu_seconds_total(&self) -> f64 {
        filter_host_metric(&self.0, "cpu_seconds_total")
    }
}

pub struct LoadAverageMetrics(Vec<Metric>);

#[Object]
impl LoadAverageMetrics {
    /// Load 1 average
    async fn load1(&self) -> f64 {
        filter_host_metric(&self.0, "load1")
    }

    /// Load 5 average
    async fn load5(&self) -> f64 {
        filter_host_metric(&self.0, "load5")
    }

    /// Load 15 average
    async fn load15(&self) -> f64 {
        filter_host_metric(&self.0, "load15")
    }
}

pub struct NetworkMetrics(Vec<Metric>);

#[Object]
impl NetworkMetrics {
    /// Total bytes received
    async fn receive_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "network_receive_bytes_total")
    }

    /// Total errors received
    async fn receive_errs_total(&self) -> f64 {
        filter_host_metric(&self.0, "network_receive_errs_total")
    }

    /// Total packets received
    async fn receive_packets_total(&self) -> f64 {
        filter_host_metric(&self.0, "network_receive_packets_total")
    }

    /// Total bytes transmitted
    async fn transmit_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "network_transmit_bytes_total")
    }

    /// Total errors transmitted
    async fn transmit_errs_total(&self) -> f64 {
        filter_host_metric(&self.0, "network_transmit_errs_total")
    }

    /// Total transmission packets dropped (Linux/Windows only)
    async fn transmit_packets_drop_total(&self) -> Option<f64> {
        if cfg!(any(target_os = "linux", windows)) {
            Some(filter_host_metric(
                &self.0,
                "network_transmit_packets_drop_total",
            ))
        } else {
            None
        }
    }

    /// Total transmission packets (Linux/Windows only)
    async fn transmit_packets_total(&self) -> Option<f64> {
        if cfg!(any(target_os = "linux", windows)) {
            Some(filter_host_metric(
                &self.0,
                "network_transmit_packets_total",
            ))
        } else {
            None
        }
    }
}

pub struct FileSystemMetrics(Vec<Metric>);

#[Object]
impl FileSystemMetrics {
    /// Free bytes
    async fn free_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "filesystem_free_bytes")
    }

    /// Total bytes
    async fn total_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "filesystem_total_bytes")
    }

    /// Used bytes
    async fn used_bytes(&self) -> f64 {
        filter_host_metric(&self.0, "filesystem_used_bytes")
    }
}

pub struct DiskMetrics(Vec<Metric>);

#[Object]
impl DiskMetrics {
    /// Total bytes read
    async fn read_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "disk_read_bytes_total")
    }

    /// Total reads completed
    async fn reads_completed_total(&self) -> f64 {
        filter_host_metric(&self.0, "disk_reads_completed_total")
    }

    /// Total bytes written
    async fn written_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "disk_written_bytes_total")
    }

    /// Total writes completed
    async fn writes_completed_total(&self) -> f64 {
        filter_host_metric(&self.0, "disk_writes_completed_total")
    }
}

pub struct TCPMetrics(Vec<Metric>);

#[Object]
impl TCPMetrics {
    /// Total TCP connections
    async fn tcp_conns_total(&self) -> f64 {
        filter_host_metric(&self.0, "tcp_connections_total")
    }

    /// Total bytes in the send queue across all connections.
    async fn tcp_tx_queued_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "tcp_tx_queued_bytes_total")
    }

    /// Total bytes in the receive queue across all connections.
    async fn tcp_rx_queued_bytes_total(&self) -> f64 {
        filter_host_metric(&self.0, "tcp_rx_queued_bytes_total")
    }
}

pub struct HostMetrics(host_metrics::HostMetrics);

impl HostMetrics {
    /// Primes the host metrics pump by passing through a new `HostMetrics`
    pub fn new() -> Self {
        let config = host_metrics::HostMetricsConfig::default();
        Self(host_metrics::HostMetrics::new(config))
    }
}

#[Object]
/// Vector host metrics
impl HostMetrics {
    /// Memory metrics
    async fn memory(&self) -> MemoryMetrics {
        let mut buffer = self.0.buffer();
        self.0.memory_metrics(&mut buffer).await;
        MemoryMetrics(buffer.metrics)
    }

    /// Swap metrics
    async fn swap(&self) -> SwapMetrics {
        let mut buffer = self.0.buffer();
        self.0.swap_metrics(&mut buffer).await;
        SwapMetrics(buffer.metrics)
    }

    /// CPU metrics
    async fn cpu(&self) -> CpuMetrics {
        let mut buffer = self.0.buffer();
        self.0.cpu_metrics(&mut buffer).await;
        CpuMetrics(buffer.metrics)
    }

    /// Load average metrics (*nix only)
    async fn load_average(&self) -> Option<LoadAverageMetrics> {
        if cfg!(unix) {
            let mut buffer = self.0.buffer();
            self.0.loadavg_metrics(&mut buffer).await;
            Some(LoadAverageMetrics(buffer.metrics))
        } else {
            None
        }
    }

    /// Network metrics
    async fn network(&self) -> NetworkMetrics {
        let mut buffer = self.0.buffer();
        self.0.network_metrics(&mut buffer).await;
        NetworkMetrics(buffer.metrics)
    }

    /// Filesystem metrics
    async fn filesystem(&self) -> FileSystemMetrics {
        let mut buffer = self.0.buffer();
        self.0.filesystem_metrics(&mut buffer).await;
        FileSystemMetrics(buffer.metrics)
    }

    /// Disk metrics
    async fn disk(&self) -> DiskMetrics {
        let mut buffer = self.0.buffer();
        self.0.disk_metrics(&mut buffer).await;
        DiskMetrics(buffer.metrics)
    }

    #[cfg(target_os = "linux")]
    /// TCP metrics
    async fn tcp(&self) -> TCPMetrics {
        let mut buffer = self.0.buffer();
        self.0.tcp_metrics(&mut buffer).await;
        TCPMetrics(buffer.metrics)
    }
}

/// Filters a [`Vec<Metric>`] by name, returning the inner `value` or 0.00 if not found
fn filter_host_metric(metrics: &[Metric], name: &str) -> f64 {
    metrics
        .iter()
        .find(|m| matches!(m.namespace(), Some(n) if n == "host") && m.name() == name)
        .map(|m| match m.value() {
            MetricValue::Gauge { value } => *value,
            MetricValue::Counter { value } => *value,
            _ => 0.00,
        })
        .unwrap_or_else(|| 0.00)
}
