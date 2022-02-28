use crate::config::SinkDescription;

pub(crate) mod common;
pub(crate) mod logs;
pub(crate) mod metrics;

use self::{logs::config::HecLogsSinkConfig, metrics::config::HecMetricsSinkConfig};

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
