#[cfg(target_os = "linux")]
use heim::memory::os::linux::MemoryExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
#[cfg(not(windows))]
use heim::memory::os::SwapExt;
use heim::units::information::byte;
use vector_lib::event::MetricTags;

use crate::internal_events::HostMetricsScrapeDetailError;

use super::HostMetrics;

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
}
