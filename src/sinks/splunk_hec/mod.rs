use crate::{event::EventRef, internal_events::TemplateRenderingFailed, template::Template};

mod conn;
pub mod logs;
pub mod metrics;

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
