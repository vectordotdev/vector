use vector_lib::{
    event::Event,
    internal_event::emit,
    partition::{PartitionDecision, Partitioner},
    sink::PreparationError,
};

use crate::{internal_events::TemplateRenderingError, template::UnconfinedTemplate};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ChroniclePartitionKey {
    pub log_type: String,
    pub namespace: Option<String>,
}

/// Partitions items based on the generated key for the given event.
pub struct ChroniclePartitioner {
    log_type: UnconfinedTemplate,
    fallback_log_type: Option<String>,
    namespace_template: Option<UnconfinedTemplate>,
}

impl ChroniclePartitioner {
    #[must_use]
    pub const fn new(
        log_type: UnconfinedTemplate,
        fallback_log_type: Option<String>,
        namespace_template: Option<UnconfinedTemplate>,
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

    fn partition(
        &self,
        item: &Self::Item,
    ) -> Result<PartitionDecision<Self::Key>, PreparationError<Self::Error>> {
        let log_type = match self.log_type.render_string(item) {
            Ok(log_type) => log_type,
            Err(error) if self.fallback_log_type.is_some() => {
                emit(TemplateRenderingError {
                    error,
                    field: Some("log_type"),
                    drop_event: false,
                });
                self.fallback_log_type.clone().expect("configured fallback")
            }
            Err(error) => {
                emit(TemplateRenderingError {
                    error: error.clone(),
                    field: Some("log_type"),
                    drop_event: true,
                });
                return Err(PreparationError::Permanent(error));
            }
        };

        let namespace = self
            .namespace_template
            .as_ref()
            .map(|namespace| {
                namespace.render_string(item).map_err(|error| {
                    emit(TemplateRenderingError {
                        error: error.clone(),
                        field: Some("namespace"),
                        drop_event: true,
                    });
                    PreparationError::Permanent(error)
                })
            })
            .transpose()?;

        Ok(PartitionDecision::Route(ChroniclePartitionKey {
            log_type,
            namespace,
        }))
    }
}
