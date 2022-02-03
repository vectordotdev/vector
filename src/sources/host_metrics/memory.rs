use chrono::Utc;
#[cfg(target_os = "linux")]
use heim::memory::os::linux::MemoryExt;
#[cfg(target_os = "macos")]
use heim::memory::os::macos::MemoryExt;
#[cfg(not(target_os = "windows"))]
use heim::memory::os::SwapExt;
use heim::units::information::byte;
use vector_common::btreemap;

use super::HostMetrics;
use crate::event::metric::Metric;

impl HostMetrics {
    pub async fn memory_metrics(&self) -> Vec<Metric> {
        match heim::memory::memory().await {
            Ok(memory) => {
                let timestamp = Utc::now();
                vec![
                    self.gauge(
                        "memory_total_bytes",
                        timestamp,
                        memory.total().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    self.gauge(
                        "memory_free_bytes",
                        timestamp,
                        memory.free().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    self.gauge(
                        "memory_available_bytes",
                        timestamp,
                        memory.available().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(any(target_os = "linux", target_os = "macos"))]
                    self.gauge(
                        "memory_active_bytes",
                        timestamp,
                        memory.active().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "linux")]
                    self.gauge(
                        "memory_buffers_bytes",
                        timestamp,
                        memory.buffers().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "linux")]
                    self.gauge(
                        "memory_cached_bytes",
                        timestamp,
                        memory.cached().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "linux")]
                    self.gauge(
                        "memory_shared_bytes",
                        timestamp,
                        memory.shared().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "linux")]
                    self.gauge(
                        "memory_used_bytes",
                        timestamp,
                        memory.used().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "macos")]
                    self.gauge(
                        "memory_inactive_bytes",
                        timestamp,
                        memory.inactive().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(target_os = "macos")]
                    self.gauge(
                        "memory_wired_bytes",
                        timestamp,
                        memory.wire().get::<byte>() as f64,
                        btreemap! {},
                    ),
                ]
            }
            Err(error) => {
                error!(message = "Failed to load memory info.", %error, internal_log_rate_secs = 60);
                vec![]
            }
        }
    }

    pub async fn swap_metrics(&self) -> Vec<Metric> {
        match heim::memory::swap().await {
            Ok(swap) => {
                let timestamp = Utc::now();
                vec![
                    self.gauge(
                        "memory_swap_free_bytes",
                        timestamp,
                        swap.free().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    self.gauge(
                        "memory_swap_total_bytes",
                        timestamp,
                        swap.total().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    self.gauge(
                        "memory_swap_used_bytes",
                        timestamp,
                        swap.used().get::<byte>() as f64,
                        btreemap! {},
                    ),
                    #[cfg(not(target_os = "windows"))]
                    self.counter(
                        "memory_swapped_in_bytes_total",
                        timestamp,
                        swap.sin().map(|swap| swap.get::<byte>()).unwrap_or(0) as f64,
                        btreemap! {},
                    ),
                    #[cfg(not(target_os = "windows"))]
                    self.counter(
                        "memory_swapped_out_bytes_total",
                        timestamp,
                        swap.sout().map(|swap| swap.get::<byte>()).unwrap_or(0) as f64,
                        btreemap! {},
                    ),
                ]
            }
            Err(error) => {
                error!(message = "Failed to load swap info.", %error, internal_log_rate_secs = 60);
                vec![]
            }
        }
    }
}
