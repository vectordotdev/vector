use crate::event::{Metric, MetricValue};
use crate::sources;
use crate::sources::host_metrics::HostMetricsConfig;
use async_graphql::Object;

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

    /// Active bytes (Linux and macOS)
    async fn active_bytes(&self) -> Option<f64> {
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            Some(filter_host_metric(&self.0, "memory_active_bytes"))
        } else {
            None
        }
    }

    /// Buffers bytes (Linux)
    async fn buffers_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_buffers_bytes"))
        } else {
            None
        }
    }

    /// Cached bytes (Linux)
    async fn cached_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_cached_bytes"))
        } else {
            None
        }
    }

    /// Shared bytes (Linux)
    async fn shared_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_shared_bytes"))
        } else {
            None
        }
    }

    /// Used bytes (Linux)
    async fn used_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "linux") {
            Some(filter_host_metric(&self.0, "memory_used_bytes"))
        } else {
            None
        }
    }

    /// Inactive bytes (macOS)
    async fn inactive_bytes(&self) -> Option<f64> {
        if cfg!(target_os = "macos") {
            Some(filter_host_metric(&self.0, "memory_inactive_bytes"))
        } else {
            None
        }
    }

    /// Wired bytes (macOS)
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
        if cfg!(not(target_os = "windows")) {
            Some(filter_host_metric(&self.0, "memory_swapped_in_bytes_total"))
        } else {
            None
        }
    }

    /// Swapped out bytes total (not available on Windows)
    async fn swapped_out_bytes_total(&self) -> Option<f64> {
        if cfg!(not(target_os = "windows")) {
            Some(filter_host_metric(
                &self.0,
                "memory_swapped_out_bytes_total",
            ))
        } else {
            None
        }
    }
}

pub struct CPUMetrics(Vec<Metric>);

#[Object]
impl CPUMetrics {
    /// CPU seconds total
    async fn cpu_seconds_total(&self) -> f64 {
        filter_host_metric(&self.0, "cpu_seconds_total")
    }
}

pub struct HostMetrics(HostMetricsConfig);

impl HostMetrics {
    /// Primes the host metrics pump by passing through a new `HostMetricsConfig`
    pub fn new() -> Self {
        Self(sources::host_metrics::HostMetricsConfig::default())
    }
}

#[Object]
/// Vector host metrics
impl HostMetrics {
    /// Memory metrics
    async fn memory(&self) -> MemoryMetrics {
        MemoryMetrics(self.0.memory_metrics().await)
    }

    /// Swap metrics
    async fn swap(&self) -> SwapMetrics {
        SwapMetrics(self.0.swap_metrics().await)
    }

    /// CPU metrics
    async fn cpu(&self) -> CPUMetrics {
        CPUMetrics(self.0.cpu_metrics().await)
    }
}

/// Filters a Vec<Metric> by name, returning the inner `value` or 0.00 if not found
fn filter_host_metric(metrics: &Vec<Metric>, name: &str) -> f64 {
    metrics
        .into_iter()
        .find(|m| m.name == format!("host_{}", name))
        .map(|m| match m.value {
            MetricValue::Gauge { value } => value,
            MetricValue::Counter { value } => value,
            _ => 0.00,
        })
        .unwrap_or_else(|| 0.00)
}
