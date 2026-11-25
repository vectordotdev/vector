use vector_lib::{
    event::Event,
    partition::{PartitionDecision, Partitioner},
    sink::PreparationError,
};

use crate::{
    sinks::util::partitioner::{PartitionError, render_key_with_fallback, render_partition_value},
    template::ConfinedTemplate,
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
    key_prefix_template: ConfinedTemplate,
    ssekms_key_id_template: Option<ConfinedTemplate>,
    dead_letter_key_prefix: Option<String>,
}

impl S3KeyPartitioner {
    #[must_use]
    pub const fn new(
        key_prefix_template: ConfinedTemplate,
        ssekms_key_id_template: Option<ConfinedTemplate>,
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
    type Key = S3PartitionKey;
    type Error = PartitionError;

    fn partition(
        &self,
        item: &Self::Item,
    ) -> Result<PartitionDecision<Self::Key>, PreparationError<Self::Error>> {
        let PartitionDecision::Route(key_prefix) = render_key_with_fallback(
            &self.key_prefix_template,
            item,
            self.dead_letter_key_prefix.as_deref(),
        )?
        else {
            return Ok(PartitionDecision::Discard);
        };

        let ssekms_key_id = if let Some(template) = &self.ssekms_key_id_template {
            let PartitionDecision::Route(key_id) =
                render_partition_value(template.render_string(item), "ssekms_key_id")?
            else {
                return Ok(PartitionDecision::Discard);
            };
            Some(key_id)
        } else {
            None
        };

        Ok(PartitionDecision::Route(S3PartitionKey {
            key_prefix,
            ssekms_key_id,
        }))
    }
}
