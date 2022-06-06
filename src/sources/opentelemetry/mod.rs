mod log;

use crate::config::SourceDescription;
use log::OpentelemetryLogConfig;

inventory::submit! {
    SourceDescription::new::<OpentelemetryLogConfig>("opentelemetry")
}
