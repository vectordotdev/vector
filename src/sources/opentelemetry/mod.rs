mod log;

use crate::config::SourceDescription;
use log::OpentelemetryConfig;

inventory::submit! {
    SourceDescription::new::<OpentelemetryConfig>("opentelemetry")
}
