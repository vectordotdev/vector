use vector_lib::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingError, template::Template};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct S3PartitionKey {
    pub key_prefix: String,
    pub ssekms_key_id: Option<String>,
}

/// Partitions items based on the generated key for the given event.
pub struct S3KeyPartitioner {
    key_prefix_template: Template,
    ssekms_key_id_template: Option<Template>,
    dead_letter_key_prefix: Option<String>,
}

impl S3KeyPartitioner {
    pub const fn new(
        key_prefix_template: Template,
        ssekms_key_id_template: Option<Template>,
        dead_letter_key_prefix: Option<String>,
    ) -> Self {
        Self {
            key_prefix_template,
            ssekms_key_id_template,
            dead_letter_key_prefix,
        }
    }
}

impl Partitioner for S3KeyPartitioner {
    type Item = Event;
    type Key = Option<S3PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let key_prefix = self
            .key_prefix_template
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
                    Err(emit!(TemplateRenderingError {
                        error,
                        field: Some("key_prefix"),
                        drop_event: true,
                    }))
                }
            })
            .ok()?;

        let ssekms_key_id = self
            .ssekms_key_id_template
            .as_ref()
            .map(|ssekms_key_id| {
                ssekms_key_id.render_string(item).map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("ssekms_key_id"),
                        drop_event: true,
                    });
                })
            })
            .transpose()
            .ok()?;
        Some(S3PartitionKey {
            key_prefix,
            ssekms_key_id,
        })
    }
}
