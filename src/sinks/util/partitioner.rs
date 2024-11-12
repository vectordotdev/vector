use vector_lib::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingError, template::Template};

/// Partitions items based on the generated key for the given event.
pub struct KeyPartitioner(Template, Option<String>);

impl KeyPartitioner {
    pub const fn new(template: Template, dead_letter_key_prefix: Option<String>) -> Self {
        Self(template, dead_letter_key_prefix)
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.0
            .render_string(item)
            .or_else(|error| {
                if let Some(dead_letter_key_prefix) = &self.1 {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("key_prefix"),
                        drop_event: false,
                    });
                    Ok(dead_letter_key_prefix.clone())
                } else {
                    Err(emit!(TemplateRenderingError {
                        error,
                        field: Some("key_prefix"),
                        drop_event: true,
                    }))
                }
            })
            .ok()
    }
}
