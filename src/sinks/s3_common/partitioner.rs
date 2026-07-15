use vector_lib::{event::Event, partition::Partitioner};

use crate::{
    internal_events::TemplateRenderingError, sinks::util::partitioner::render_key_with_fallback,
    template::Template,
};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct S3PartitionKey {
    pub key_prefix: String,
    pub ssekms_key_id: Option<String>,
}

/// Partitions items based on the generated key for the given event.
///
/// If the template was built with a confinement check (via
/// [`Template::confine`][crate::template::Template::confine]),
/// keys that escape the base prefix are dropped as intentional security discards.
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
        let key_prefix = render_key_with_fallback(
            &self.key_prefix_template,
            item,
            self.dead_letter_key_prefix.as_deref(),
        )?;

        let ssekms_key_id = self
            .ssekms_key_id_template
            .as_ref()
            .map(|t| {
                t.render_string(item).map_err(|error| {
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
