mod log;

use log::OpentelemetryLogConfig;
use crate::config::SourceDescription;

inventory::submit! {
    SourceDescription::new::<OpentelemetryLogConfig>("otel_log")
}

