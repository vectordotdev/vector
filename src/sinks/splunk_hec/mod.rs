use crate::config::SinkDescription;

mod common;
mod conn;
pub mod logs;
pub mod metrics;

use logs::config::HecSinkLogsConfig;

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
