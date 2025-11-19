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
    type Key = ChroniclePartitionKey;
    type Error = crate::template::TemplateRenderingError;

    fn partition(&self, item: &Self::Item) -> Result<Self::Key, Self::Error> {
        let log_type = self.log_type.render_string(item).or_else(|error| {
            if let Some(fallback_log_type) = &self.fallback_log_type {
                emit!(TemplateRenderingError {
                    error,
                    field: Some("log_type"),
                    drop_event: false,
                });
                Ok(fallback_log_type.clone())
            } else {
                emit!(TemplateRenderingError {
                    error: error.clone(),
                    field: Some("log_type"),
                    drop_event: true,
                });
                Err(error)
            }
        })?;

        let namespace = self
            .namespace_template
            .as_ref()
            .map(|namespace| {
                namespace.render_string(item).inspect_err(|error| {
                    emit!(TemplateRenderingError {
                        error: error.clone(),
                        field: Some("namespace"),
                        drop_event: true,
                    });
                })
            })
            .transpose()?;

        Ok(ChroniclePartitionKey {
            log_type,
            namespace,
        })
    }
}
