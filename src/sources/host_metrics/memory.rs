#[cfg(not(windows))]
use heim::memory::os::SwapExt;
#[cfg(target_os = "linux")]
use heim::memory::os::linux::MemoryExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
use heim::units::information::byte;
use vector_lib::event::MetricTags;

use super::HostMetrics;
use crate::internal_events::HostMetricsScrapeDetailError;

#[cfg(target_os = "linux")]
const OOM_KILL: &str = "oom_kill";

impl HostMetrics {
    pub async fn memory_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "memory";
        match heim::memory::memory().await {
            Ok(memory) => {
                output.gauge(
                    "memory_total_bytes",
                    memory.total().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "memory_free_bytes",
                    memory.free().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "memory_available_bytes",
                    memory.available().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                output.gauge(
                    "memory_active_bytes",
                    memory.active().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    "memory_buffers_bytes",
                    memory.buffers().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    "memory_cached_bytes",
                    memory.cached().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    "memory_shared_bytes",
                    memory.shared().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    "memory_used_bytes",
                    memory.used().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "macos")]
                output.gauge(
                    "memory_inactive_bytes",
                    memory.inactive().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "macos")]
                output.gauge(
                    "memory_wired_bytes",
                    memory.wire().get::<byte>() as f64,
                    MetricTags::default(),
                );
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load memory info.",
                    error,
                });
            }
        }
    }

    pub async fn swap_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "memory";
        match heim::memory::swap().await {
            Ok(swap) => {
                output.gauge(
                    "memory_swap_free_bytes",
                    swap.free().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "memory_swap_total_bytes",
                    swap.total().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    "memory_swap_used_bytes",
                    swap.used().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(not(windows))]
                output.counter(
                    "memory_swapped_in_bytes_total",
                    swap.sin().map(|swap| swap.get::<byte>()).unwrap_or(0) as f64,
                    MetricTags::default(),
                );
                #[cfg(not(windows))]
                output.counter(
                    "memory_swapped_out_bytes_total",
                    swap.sout().map(|swap| swap.get::<byte>()).unwrap_or(0) as f64,
                    MetricTags::default(),
                );
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load swap info.",
                    error,
                });
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub async fn vmstat_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "memory";

        // Spawn blocking task to avoid blocking the async runtime with synchronous I/O
        let result = tokio::task::spawn_blocking(procfs::vmstat)
            .await
            .unwrap_or_else(|join_error| {
                Err(procfs::ProcError::Other(format!(
                    "Failed to join blocking task: {}",
                    join_error
                )))
            });

        match result {
            Ok(stats) => {
                if let Some(&oom_kill) = stats.get("oom_kill") {
                    output.counter(OOM_KILL, oom_kill as f64, MetricTags::default());
                }
            }
            Err(error) => {
                emit!(HostMetricsScrapeDetailError {
                    message: "Failed to load vmstat info.",
                    error,
                });
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use crate::event::metric::MetricValue;
    use crate::sources::host_metrics::{HostMetrics, HostMetricsConfig, MetricsBuffer};

    use super::OOM_KILL;

    #[tokio::test]
    async fn generates_vmstat_oom_kill_metric() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .vmstat_metrics(&mut buffer)
            .await;
        let metrics = buffer.metrics;

        assert_eq!(metrics.len(), 1);

        let metric = &metrics[0];
        assert_eq!(metric.name(), OOM_KILL);
        assert!(
            matches!(metric.value(), MetricValue::Counter { .. }),
            "oom_kill metric should be a counter"
        );

        let tags = metric.tags().expect("metric must have tags");
        assert_eq!(tags.get("collector"), Some("memory"));
    }
}
