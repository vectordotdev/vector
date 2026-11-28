use vector_lib::{
    event::Event,
    internal_event::emit,
    partition::{PartitionDecision, Partitioner},
    sink::PreparationError,
};

use crate::{
    internal_events::{KeyOutsideBasePrefixError, TemplateRenderingError},
    template::{ConfinedTemplate, TemplateRenderingError as TplRenderError},
};

/// Render `template` against `event` and apply standard key-prefix error handling.
///
/// Routes to the rendered key or the configured fallback. Confinement violations are
/// intentional discards. A render failure without a fallback is a permanent failure.
pub(crate) fn render_key_with_fallback(
    template: &ConfinedTemplate,
    event: &Event,
    dead_letter: Option<&str>,
) -> Result<PartitionDecision<String>, PreparationError<TplRenderError>> {
    match template.render_string(event) {
        Ok(key) => Ok(PartitionDecision::Route(key)),
        Err(TplRenderError::Confined {
            rendered_preview,
            rendered_len,
            message,
        }) => {
            emit(KeyOutsideBasePrefixError {
                key_preview: &rendered_preview,
                key_len: rendered_len,
                message: &message,
            });
            Ok(PartitionDecision::Discard)
        }
        Err(error) => {
            if let Some(fallback) = dead_letter {
                emit(TemplateRenderingError {
                    error,
                    field: Some("key_prefix"),
                    drop_event: false,
                });
                Ok(PartitionDecision::Route(fallback.to_owned()))
            } else {
                render_partition_value(Err(error), "key_prefix")
            }
        }
    }
}

/// Reports a required routing field's render error. Confinement failures discard the event.
pub(crate) fn render_partition_value<T>(
    result: Result<T, TplRenderError>,
    field: &'static str,
) -> Result<PartitionDecision<T>, PreparationError<TplRenderError>> {
    match result {
        Ok(value) => Ok(PartitionDecision::Route(value)),
        Err(error) => {
            if matches!(error, TplRenderError::Confined { .. }) {
                emit(TemplateRenderingError {
                    error,
                    field: Some(field),
                    drop_event: true,
                });
                Ok(PartitionDecision::Discard)
            } else {
                emit(TemplateRenderingError {
                    error: error.clone(),
                    field: Some(field),
                    drop_event: true,
                });
                Err(PreparationError::Permanent(error))
            }
        }
    }
}

/// Partitions items based on the generated key for the given event.
///
/// If the template was built with a confinement check (via
/// [`Template::confine`][crate::template::Template::confine]),
/// keys that escape the base prefix are dropped as intentional security discards.
pub struct KeyPartitioner {
    key_prefix_template: ConfinedTemplate,
    dead_letter_key_prefix: Option<String>,
}

impl KeyPartitioner {
    #[must_use]
    pub const fn new(
        key_prefix_template: ConfinedTemplate,
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
    type Error = TplRenderError;

    fn partition(
        &self,
        item: &Self::Item,
    ) -> Result<PartitionDecision<Self::Key>, PreparationError<Self::Error>> {
        render_key_with_fallback(
            &self.key_prefix_template,
            item,
            self.dead_letter_key_prefix.as_deref(),
        )
    }
}
