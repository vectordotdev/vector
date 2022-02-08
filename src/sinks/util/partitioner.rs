use vector_core::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingFailed, template::Template};

/// Partitions items based on the generated key for the given event.
pub struct KeyPartitioner(Template);

impl KeyPartitioner {
    pub const fn new(template: Template) -> Self {
        Self(template)
    }
}

impl Partitioner for KeyPartitioner {
    type Item = Event;
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        self.0
            .render_string(item)
            .map_err(|error| {
                emit!(&TemplateRenderingFailed {
                    error,
                    field: Some("key_prefix"),
                    drop_event: true,
                });
            })
            .ok()
    }
}
