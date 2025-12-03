use vector_lib::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingError, template::Template};

/// Partitions items based on the generated key for the given event.
pub struct KeyPartitioner {
    key_prefix_template: Template,
    dead_letter_key_prefix: Option<String>,
}

impl KeyPartitioner {
    pub const fn new(
        key_prefix_template: Template,
        dead_letter_key_prefix: Option<String>,
    ) -> Self {
        Self {
            key_prefix_template,
            dead_letter_key_prefix,
        }
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = String;
    type Error = crate::template::TemplateRenderingError;

    fn partition(&self, item: &Self::Item) -> Result<Self::Key, Self::Error> {
        self.key_prefix_template
            .render_string(item)
            .or_else(|error| {
                if let Some(dead_letter_key_prefix) = &self.dead_letter_key_prefix {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("key_prefix"),
                        drop_event: false,
                    });
                    Ok(dead_letter_key_prefix.clone())
                } else {
                    emit!(TemplateRenderingError {
                        error: error.clone(),
                        field: Some("key_prefix"),
                        drop_event: true,
                    });
                    Err(error)
                }
            })
    }
}
