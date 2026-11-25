use std::future;

use snafu::{IntoError, Snafu};
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

/// A required routing field failed to render, so the event cannot be partitioned.
///
/// Partition helpers return this without emitting anything. The caller that consumes the
/// finalized partition result reports it exactly once, typically via
/// [`report_partition_error`].
#[derive(Debug, Snafu)]
#[snafu(display("Failed to render `{field}` template: {source}"))]
pub struct PartitionError {
    field: &'static str,
    source: TplRenderError,
}

impl PartitionError {
    /// Wraps the render `error` of the routing template configured as `field`.
    pub(crate) fn new(field: &'static str, error: TplRenderError) -> Self {
        PartitionSnafu { field }.into_error(error)
    }
}

/// Reports a partition failure and drops the event; passes successful values through.
///
/// Emits [`TemplateRenderingError`] with the failing field and `drop_event: true` for
/// `Err`, yielding `None`. Returns a ready future so it can be passed directly to
/// `StreamExt::filter_map`.
pub fn report_partition_error<T>(result: Result<T, PartitionError>) -> future::Ready<Option<T>> {
    future::ready(match result {
        Ok(value) => Some(value),
        Err(PartitionError { field, source }) => {
            emit(TemplateRenderingError {
                error: source,
                field: Some(field),
                drop_event: true,
            });
            None
        }
    })
}

/// Render `template` against `event` and apply standard key-prefix error handling.
///
/// Routes to the rendered key or the configured fallback. Confinement violations are
/// intentional discards. A render failure without a fallback is a permanent failure that
/// is returned unreported; see [`report_partition_error`].
pub(crate) fn render_key_with_fallback(
    template: &ConfinedTemplate,
    event: &Event,
    dead_letter: Option<&str>,
) -> Result<PartitionDecision<String>, PreparationError<PartitionError>> {
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

/// Converts a required routing field's render result into a partition decision.
///
/// Confinement failures are reported here and discard the event. Other render failures
/// are returned as permanent [`PartitionError`]s without being reported; the caller
/// reports them, typically via [`report_partition_error`].
pub(crate) fn render_partition_value<T>(
    result: Result<T, TplRenderError>,
    field: &'static str,
) -> Result<PartitionDecision<T>, PreparationError<PartitionError>> {
    match result {
        Ok(value) => Ok(PartitionDecision::Route(value)),
        Err(error @ TplRenderError::Confined { .. }) => {
            emit(TemplateRenderingError {
                error,
                field: Some(field),
                drop_event: true,
            });
            Ok(PartitionDecision::Discard)
        }
        Err(error) => Err(PreparationError::Permanent(PartitionError::new(
            field, error,
        ))),
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
    type Error = PartitionError;

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
