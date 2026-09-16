#[cfg(not(windows))]
use heim::memory::os::SwapExt;
#[cfg(target_os = "linux")]
use heim::memory::os::linux::MemoryExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
use heim::units::information::byte;
#[cfg(not(windows))]
use vector_lib::internal_event::CounterName;
use vector_lib::{event::MetricTags, internal_event::GaugeName};

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

    #[cfg(target_os = "linux")]
    pub async fn vmstat_metrics(&self, output: &mut super::MetricsBuffer) {
        output.name = "memory";

        // Spawn blocking task to avoid blocking the async runtime with synchronous I/O
        let result = tokio::task::spawn_blocking(procfs::vmstat)
            .await
            .unwrap_or_else(|join_error| {
                Err(procfs::ProcError::Other(format!(
                    "Failed to join blocking task: {join_error}"
                )))
            });

        match result {
            Ok(stats) => {
                if let Some(&oom_kill) = stats.get("oom_kill") {
                    output.counter(
                        CounterName::MemoryOomKillEventsTotal,
                        oom_kill as f64,
                        MetricTags::default(),
                    );
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
    use vector_lib::internal_event::CounterName;

    use crate::event::metric::MetricValue;
    use crate::sources::host_metrics::{HostMetrics, HostMetricsConfig, MetricsBuffer};

    #[tokio::test]
    async fn generates_vmstat_oom_kill_metric() {
        let mut buffer = MetricsBuffer::new(None);
        HostMetrics::new(HostMetricsConfig::default())
            .vmstat_metrics(&mut buffer)
            .await;
        let metrics = buffer.into_metrics();

        assert_eq!(metrics.len(), 1);

        let metric = &metrics[0];
        assert_eq!(
            metric.name(),
            CounterName::MemoryOomKillEventsTotal.as_str()
        );
        assert!(
            matches!(metric.value(), MetricValue::Counter { .. }),
            "memory_oom_kill_events_total metric should be a counter"
        );

        let tags = metric.tags().expect("metric must have tags");
        assert_eq!(tags.get("collector"), Some("memory"));
    }
}
