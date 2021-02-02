use super::InternalEvent;
use crate::template::TemplateRenderingError;
use metrics::counter;

pub struct TemplateRenderingFailed<'a> {
    pub field: Option<&'a str>,
    pub drop_event: bool,
    pub error: TemplateRenderingError,
}

impl<'a> InternalEvent for TemplateRenderingFailed<'a> {
    fn emit_logs(&self) {
        let mut msg = "Failed to render template".to_owned();
        if let Some(field) = self.field {
            msg.push_str(&format!(" for \"{}\"", field));
        }
        if self.drop_event {
            msg.push_str("; discarding event");
        }
        msg.push('.');
        warn!(message = %msg, error = %self.error, internal_log_rate_secs = 30);
    }

    fn emit_metrics(&self) {
        counter!("template_rendering_errors_total", 1);
        if self.drop_event {
            counter!("events_discarded_total", 1);
        }
    }
}
