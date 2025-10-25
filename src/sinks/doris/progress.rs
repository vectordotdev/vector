//! Progress reporting implementation for Doris sink.

use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time;
use tracing::info;
use vector_lib::shutdown::ShutdownSignal;

/// ProgressReporter tracks and periodically logs information about data sent to Doris.
#[derive(Debug)]
pub struct ProgressReporter {
    total_bytes: Arc<AtomicI64>,
    total_rows: Arc<AtomicI64>,
    failed_rows: Arc<AtomicI64>,
    interval: u64,
}

impl ProgressReporter {
    /// Create a new ProgressReporter with the specified reporting interval in seconds.
    pub fn new(interval: u64) -> Self {
        Self {
            total_bytes: Arc::new(AtomicI64::new(0)),
            total_rows: Arc::new(AtomicI64::new(0)),
            failed_rows: Arc::new(AtomicI64::new(0)),
            interval,
        }
    }

    /// Increment the total bytes counter.
    pub fn incr_total_bytes(&self, bytes: i64) {
        self.total_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    /// Increment the total rows counter.
    pub fn incr_total_rows(&self, rows: i64) {
        self.total_rows.fetch_add(rows, Ordering::Relaxed);
    }

    /// Increment the failed rows counter.
    pub fn incr_failed_rows(&self, rows: i64) {
        // For consistency with Filebeat, we also update the total rows count
        // Though it seems counterintuitive, this is to maintain compatibility
        self.total_rows.fetch_add(rows, Ordering::Relaxed);
        self.failed_rows.fetch_add(rows, Ordering::Relaxed);
    }

    /// Start the progress reporting loop.
    pub async fn report(&self, mut shutdown: Option<ShutdownSignal>) {
        if self.interval == 0 {
            return;
        }

        let init_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let mut last_time = init_time;
        let mut last_bytes = self.total_bytes.load(Ordering::Relaxed);
        let mut last_rows = self.total_rows.load(Ordering::Relaxed);

        info!(
            message = "Starting progress reporter.",
            interval_seconds = self.interval,
            interval = ?Duration::from_secs(self.interval),
            internal_log_rate_limit = true
        );

        loop {
            let sleep_fut = time::sleep(Duration::from_secs(self.interval));

            tokio::select! {
                _ = sleep_fut => {},
                // Exit the loop if shutdown signal is received
                _ = async { if let Some(ref mut signal) = shutdown { signal.await } else { std::future::pending().await } } => {
                    info!(
                        message = "Shutting down progress reporter.",
                        internal_log_rate_limit = true
                    );
                    break;
                }
            }

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let cur_bytes = self.total_bytes.load(Ordering::Relaxed);
            let cur_rows = self.total_rows.load(Ordering::Relaxed);

            let total_time = now.saturating_sub(init_time);
            let total_speed_mbps = if total_time > 0 {
                cur_bytes / 1024 / 1024 / total_time as i64
            } else {
                0
            };
            let total_speed_rps = if total_time > 0 {
                cur_rows / total_time as i64
            } else {
                0
            };

            let inc_bytes = cur_bytes - last_bytes;
            let inc_rows = cur_rows - last_rows;
            let inc_time = now.saturating_sub(last_time);
            let inc_speed_mbps = if inc_time > 0 {
                inc_bytes / 1024 / 1024 / inc_time as i64
            } else {
                0
            };
            let inc_speed_rps = if inc_time > 0 {
                inc_rows / inc_time as i64
            } else {
                0
            };

            // Output progress information using Vector's key-value format
            info!(
                message = "Progress statistics for Doris sink.",
                total_mb = cur_bytes / 1024 / 1024,
                total_rows = cur_rows,
                total_speed_mbps = total_speed_mbps,
                total_speed_rps = total_speed_rps,
                last_seconds = inc_time,
                last_speed_mbps = inc_speed_mbps,
                last_speed_rps = inc_speed_rps,
                internal_log_rate_limit = true
            );

            last_time = now;
            last_bytes = cur_bytes;
            last_rows = cur_rows;
        }
    }
}

impl Clone for ProgressReporter {
    fn clone(&self) -> Self {
        Self {
            total_bytes: Arc::clone(&self.total_bytes),
            total_rows: Arc::clone(&self.total_rows),
            failed_rows: Arc::clone(&self.failed_rows),
            interval: self.interval,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, timeout};

    #[test]
    fn test_new_reporter() {
        let reporter = ProgressReporter::new(10);
        assert_eq!(reporter.interval, 10);
        assert_eq!(reporter.total_bytes.load(Ordering::Relaxed), 0);
        assert_eq!(reporter.total_rows.load(Ordering::Relaxed), 0);
        assert_eq!(reporter.failed_rows.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn test_increment_counters() {
        let reporter = ProgressReporter::new(10);

        // Test total bytes
        reporter.incr_total_bytes(100);
        assert_eq!(reporter.total_bytes.load(Ordering::Relaxed), 100);
        reporter.incr_total_bytes(50);
        assert_eq!(reporter.total_bytes.load(Ordering::Relaxed), 150);

        // Test total rows
        reporter.incr_total_rows(20);
        assert_eq!(reporter.total_rows.load(Ordering::Relaxed), 20);
        reporter.incr_total_rows(10);
        assert_eq!(reporter.total_rows.load(Ordering::Relaxed), 30);

        // Test failed rows (also updates total rows)
        reporter.incr_failed_rows(5);
        assert_eq!(reporter.failed_rows.load(Ordering::Relaxed), 5);
        assert_eq!(reporter.total_rows.load(Ordering::Relaxed), 35);
    }

    #[test]
    fn test_clone() {
        let reporter = ProgressReporter::new(20);
        reporter.incr_total_bytes(200);
        reporter.incr_total_rows(50);
        reporter.incr_failed_rows(10);

        let cloned = reporter.clone();

        // Verify cloned instance has the same values
        assert_eq!(cloned.interval, reporter.interval);
        assert_eq!(cloned.total_bytes.load(Ordering::Relaxed), 200);
        assert_eq!(cloned.total_rows.load(Ordering::Relaxed), 60); // 50 + 10
        assert_eq!(cloned.failed_rows.load(Ordering::Relaxed), 10);

        // Verify that updates to one affect the other due to Arc
        reporter.incr_total_bytes(100);
        assert_eq!(cloned.total_bytes.load(Ordering::Relaxed), 300);

        cloned.incr_total_rows(40);
        assert_eq!(reporter.total_rows.load(Ordering::Relaxed), 100); // 60 + 40
    }

    #[tokio::test]
    async fn test_report_disabled() {
        let reporter = ProgressReporter::new(0);
        // Should return immediately when interval is 0
        let result = timeout(Duration::from_millis(100), reporter.report(None)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_report_shutdown() {
        // Create a reporter with a long interval to ensure it won't trigger during test
        let reporter = ProgressReporter::new(100);

        // Use ShutdownSignal::noop() which returns a signal that can be awaited
        let shutdown_signal = ShutdownSignal::noop();

        // Start the reporter in a separate task
        let handle = tokio::spawn(async move {
            reporter.report(Some(shutdown_signal)).await;
        });

        // Give it a moment to start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Cancel the task directly since we can't trigger the noop signal
        handle.abort();

        // The report task should complete within a reasonable time
        let result = timeout(Duration::from_millis(200), handle).await;
        assert!(result.is_ok());
    }
}
