use crate::config::SinkDescription;

pub mod common;
pub mod logs;
pub mod metrics;

use self::logs::config::HecLogsSinkConfig;
use self::metrics::config::HecMetricsSinkConfig;

// legacy
inventory::submit! {
    SinkDescription::new::<HecLogsSinkConfig>("splunk_hec")
}

inventory::submit! {
    SinkDescription::new::<HecLogsSinkConfig>("splunk_hec_logs")
}

inventory::submit! {
    SinkDescription::new::<HecMetricsSinkConfig>("splunk_hec_metrics")
}
