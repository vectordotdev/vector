use crate::config::SinkDescription;

mod common;
pub mod logs;
pub mod metrics;

use self::logs::config::HecSinkLogsConfig;
use self::metrics::config::HecMetricsSinkConfig;

// legacy
inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec")
}

inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec_logs")
}

inventory::submit! {
    SinkDescription::new::<HecMetricsSinkConfig>("splunk_hec_metrics")
}
