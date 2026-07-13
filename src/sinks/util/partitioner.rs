use vector_lib::{event::Event, partition::Partitioner};

use crate::{
    internal_events::{KeyOutsideBasePrefixError, TemplateRenderingError},
    template::{Template, TemplateRenderingError as TplRenderError},
};

/// Render `template` against `event` and apply standard key-prefix error handling.
///
/// Three outcomes:
/// - `Ok` → `Some(rendered_key)`
/// - `Confined` error → emit [`KeyOutsideBasePrefixError`], return `None` (intentional drop)
/// - Other render error → emit [`TemplateRenderingError`]:
///   - with `dead_letter` set: return `Some(dead_letter)` (warning, no drop)
///   - without `dead_letter`:  return `None` (error, drop)
pub(crate) fn render_key_with_fallback(
    template: &Template,
    event: &Event,
    dead_letter: Option<&str>,
) -> Option<String> {
    match template.render_string(event) {
        Ok(key) => Some(key),
        Err(TplRenderError::Confined { rendered, message }) => {
            emit!(KeyOutsideBasePrefixError {
                key: &rendered,
                message: &message,
            });
            None
        }
        Err(error) => {
            if let Some(fallback) = dead_letter {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("key_prefix"),
                    drop_event: false,
                });
                Some(fallback.to_owned())
            } else {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("key_prefix"),
                    drop_event: true,
                });
                None
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
    type Key = Option<String>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        render_key_with_fallback(
            &self.key_prefix_template,
            item,
            self.dead_letter_key_prefix.as_deref(),
        )
    }
}
