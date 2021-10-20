use crate::config::SinkDescription;

pub mod common;
mod conn;
pub mod logs;
pub mod logs_new;
pub mod metrics;

use logs_new::config::HecSinkLogsConfig;

// legacy
inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec")
}

inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec_logs")
}

inventory::submit! {
    SinkDescription::new::<metrics::HecSinkMetricsConfig>("splunk_hec_metrics")
}
