use vector_lib::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingError, template::Template};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct S3PartitionKey {
    /// Either the rendered `key_prefix` (when `is_full_key` is `false`) or the rendered full
    /// object key (when `is_full_key` is `true`).
    pub key_prefix: String,
    /// When `true`, `key_prefix` is the complete S3 object key — the request builder must use
    /// it verbatim and skip the implicit timestamp / UUID / extension suffix.
    pub is_full_key: bool,
    pub ssekms_key_id: Option<String>,
}

/// Partitions items based on the generated key for the given event.
pub struct S3KeyPartitioner {
    key_prefix_template: Template,
    /// When set, this template renders the full object key and overrides `key_prefix_template`.
    key_template: Option<Template>,
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
            key_template: None,
            ssekms_key_id_template,
            dead_letter_key_prefix,
        }
    }

    pub fn with_key_template(mut self, key_template: Option<Template>) -> Self {
        self.key_template = key_template;
        self
    }
}

impl Partitioner for S3KeyPartitioner {
    type Item = Event;
    type Key = Option<S3PartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let (key_prefix, is_full_key) = if let Some(key_template) = &self.key_template {
            let rendered = key_template
                .render_string(item)
                .map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("key"),
                        drop_event: true,
                    });
                })
                .ok()?;
            (rendered, true)
        } else {
            let rendered = self
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
            (rendered, false)
        };

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
            is_full_key,
            ssekms_key_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use vector_lib::event::{Event, LogEvent};

    use super::*;

    #[test]
    fn renders_key_prefix_when_no_key_template() {
        let prefix = Template::try_from("logs/date=%F").unwrap();
        let partitioner = S3KeyPartitioner::new(prefix, None, None);

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("host", "h-1");

        let key = partitioner.partition(&event).unwrap();
        assert!(key.key_prefix.starts_with("logs/date="));
        assert!(!key.is_full_key);
    }

    #[test]
    fn renders_full_key_when_key_template_present() {
        let prefix = Template::try_from("ignored/").unwrap();
        let key_template = Template::try_from("custom/{{ host }}.log").unwrap();
        let partitioner =
            S3KeyPartitioner::new(prefix, None, None).with_key_template(Some(key_template));

        let mut event = Event::Log(LogEvent::from("hello"));
        event.as_mut_log().insert("host", "h-1");

        let key = partitioner.partition(&event).unwrap();
        assert_eq!(key.key_prefix, "custom/h-1.log");
        assert!(key.is_full_key);
    }

    #[test]
    fn key_template_rendering_failure_drops_event() {
        let prefix = Template::try_from("ignored/").unwrap();
        let key_template = Template::try_from("custom/{{ missing }}.log").unwrap();
        let partitioner =
            S3KeyPartitioner::new(prefix, None, None).with_key_template(Some(key_template));

        let event = Event::Log(LogEvent::from("hello"));
        assert!(partitioner.partition(&event).is_none());
    }
}
