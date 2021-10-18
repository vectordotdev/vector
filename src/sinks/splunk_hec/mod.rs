use crate::{
    config::SinkDescription, event::EventRef, internal_events::TemplateRenderingFailed,
    template::Template,
};

pub mod config;
mod conn;
pub mod logs;
pub mod metrics;
pub mod sink;

use self::{config::HecSinkMetricsConfig, config::HecSinkLogsConfig};

// legacy
inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec")
}

inventory::submit! {
    SinkDescription::new::<HecSinkLogsConfig>("splunk_hec_logs")
}

inventory::submit! {
    SinkDescription::new::<HecSinkMetricsConfig>("splunk_hec_metrics")
}

fn render_template_string<'a>(
    template: &Template,
    event: impl Into<EventRef<'a>>,
    field_name: &str,
) -> Option<String> {
    template
        .render_string(event)
        .map_err(|error| {
            emit!(&TemplateRenderingFailed {
                error,
                field: Some(field_name),
                drop_event: false
            });
        })
        .ok()
}
