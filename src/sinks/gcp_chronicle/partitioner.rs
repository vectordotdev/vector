use vector_lib::{event::Event, partition::Partitioner};

use crate::{internal_events::TemplateRenderingError, template::Template};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChroniclePartitionKey {
    pub log_type: String,
    pub namespace: Option<String>,
}

/// Partitions items based on the generated key for the given event.
pub struct ChroniclePartitioner {
    log_type: Template,
    fallback_log_type: Option<String>,
    namespace_template: Option<Template>,
}

impl ChroniclePartitioner {
    pub const fn new(
        log_type: Template,
        fallback_log_type: Option<String>,
        namespace_template: Option<Template>,
    ) -> Self {
        Self {
            log_type,
            fallback_log_type,
            namespace_template,
        }
    }
}

impl Partitioner for ChroniclePartitioner {
    type Item = Event;
    type Key = Option<ChroniclePartitionKey>;

    fn partition(&self, item: &Self::Item) -> Self::Key {
        let log_type = self
            .log_type
            .render_string(item)
            .or_else(|error| {
                if let Some(fallback_log_type) = &self.fallback_log_type {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("log_type"),
                        drop_event: false,
                    });
                    Ok(fallback_log_type.clone())
                } else {
                    Err(emit!(TemplateRenderingError {
                        error,
                        field: Some("log_type"),
                        drop_event: true,
                    }))
                }
            })
            .ok()?;

        let namespace = self
            .namespace_template
            .as_ref()
            .map(|namespace| {
                namespace.render_string(item).map_err(|error| {
                    emit!(TemplateRenderingError {
                        error,
                        field: Some("namespace"),
                        drop_event: true,
                    });
                })
            })
            .transpose()
            .ok()?;
        Some(ChroniclePartitionKey {
            log_type,
            namespace,
        })
    }
}
