//! Progress reporting implementation for Doris sink.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
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
            interval = ?Duration::from_secs(self.interval)
        );

        loop {
            let sleep_fut = time::sleep(Duration::from_secs(self.interval));

            tokio::select! {
                _ = sleep_fut => {},
                // Exit the loop if shutdown signal is received
                _ = async { if let Some(ref mut signal) = shutdown { signal.await } else { std::future::pending().await } } => {
                    info!(
                        message = "Shutting down progress reporter."
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
                last_speed_rps = inc_speed_rps
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
