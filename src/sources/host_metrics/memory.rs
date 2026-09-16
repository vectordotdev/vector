#[cfg(not(windows))]
use heim::memory::os::SwapExt;
#[cfg(target_os = "linux")]
use heim::memory::os::linux::MemoryExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
use heim::units::information::byte;
use vector_lib::{
    event::MetricTags,
    internal_event::{CounterName, GaugeName},
};

use super::HostMetrics;
use crate::internal_events::HostMetricsScrapeDetailError;

impl HostMetrics {
    pub async fn memory_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "memory";
        match heim::memory::memory().await {
            Ok(memory) => {
                output.gauge(
                    GaugeName::MemoryTotalBytes,
                    memory.total().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    GaugeName::MemoryFreeBytes,
                    memory.free().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    GaugeName::MemoryAvailableBytes,
                    memory.available().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(any(target_os = "linux", target_os = "macos"))]
                output.gauge(
                    GaugeName::MemoryActiveBytes,
                    memory.active().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    GaugeName::MemoryBuffersBytes,
                    memory.buffers().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    GaugeName::MemoryCachedBytes,
                    memory.cached().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    GaugeName::MemorySharedBytes,
                    memory.shared().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "linux")]
                output.gauge(
                    GaugeName::MemoryUsedBytes,
                    memory.used().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "macos")]
                output.gauge(
                    GaugeName::MemoryInactiveBytes,
                    memory.inactive().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(target_os = "macos")]
                output.gauge(
                    GaugeName::MemoryWiredBytes,
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
                    GaugeName::MemorySwapFreeBytes,
                    swap.free().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    GaugeName::MemorySwapTotalBytes,
                    swap.total().get::<byte>() as f64,
                    MetricTags::default(),
                );
                output.gauge(
                    GaugeName::MemorySwapUsedBytes,
                    swap.used().get::<byte>() as f64,
                    MetricTags::default(),
                );
                #[cfg(not(windows))]
                output.counter(
                    CounterName::MemorySwappedInBytesTotal,
                    swap.sin().map(|swap| swap.get::<byte>()).unwrap_or(0) as f64,
                    MetricTags::default(),
                );
                #[cfg(not(windows))]
                output.counter(
                    CounterName::MemorySwappedOutBytesTotal,
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
