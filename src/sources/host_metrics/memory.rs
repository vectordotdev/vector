#[cfg(target_os = "linux")]
use std::collections::HashMap;

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

#[cfg(target_os = "linux")]
fn record_vmstat_metrics(stats: &HashMap<String, i64>, output: &mut super::MetricsBuffer) {
    output.name = "memory";

    if let Some(&oom_kill) = stats.get("oom_kill") {
        output.counter(
            CounterName::MemoryOomKillEventsTotal,
            oom_kill as f64,
            MetricTags::default(),
        );
    }
}

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
        // Spawn blocking task to avoid blocking the async runtime with synchronous I/O
        let result = tokio::task::spawn_blocking(procfs::vmstat)
            .await
            .unwrap_or_else(|join_error| {
                Err(procfs::ProcError::Other(format!(
                    "Failed to join blocking task: {join_error}"
                )))
            });

        match result {
            Ok(stats) => record_vmstat_metrics(&stats, output),
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
    use std::collections::HashMap;

    use vector_lib::internal_event::CounterName;

    use super::record_vmstat_metrics;
    use crate::event::metric::MetricValue;
    use crate::sources::host_metrics::MetricsBuffer;

    #[test]
    fn generates_vmstat_oom_kill_metric() {
        let stats = HashMap::from([("oom_kill".to_owned(), 7)]);
        let mut buffer = MetricsBuffer::new(None);
        record_vmstat_metrics(&stats, &mut buffer);
        let metrics = buffer.into_metrics();

        assert_eq!(metrics.len(), 1);

        let metric = &metrics[0];
        assert_eq!(
            metric.name(),
            CounterName::MemoryOomKillEventsTotal.as_str()
        );
        assert_eq!(metric.value(), &MetricValue::Counter { value: 7.0 });

        let tags = metric.tags().expect("metric must have tags");
        assert_eq!(tags.get("collector"), Some("memory"));
    }
}
