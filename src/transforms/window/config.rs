use vector_lib::config::{clone_input_definitions, LogNamespace};
use vector_lib::configurable::configurable_component;

use crate::{
    conditions::AnyCondition,
    config::{
        DataType, GenerateConfig, Input, OutputId, TransformConfig, TransformContext,
        TransformOutput,
    },
    schema,
    transforms::Transform,
};

use super::transform::Window;

/// Configuration for the `window` transform.
#[configurable_component(transform(
    "window",
    "Apply a buffered sliding window over the stream of events and flush it based on supplied criteria"
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct WindowConfig {
    /// A condition used to pass events through the transform without buffering
    pub pass_when: Option<AnyCondition>,
    /// A condition used to flush the events
    pub flush_when: AnyCondition,
    /// The maximum number of events to keep before the event matched by the flush_when condition
    pub events_before: Option<usize>,
    /// The maximum number of events to keep after the event matched by the flush_when condition
    pub events_after: Option<usize>,
}

impl GenerateConfig for WindowConfig {
    fn generate_config() -> toml::Value {
        toml::from_str(r#"flush_when = ".message == \"value\"""#).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "window")]
impl TransformConfig for WindowConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(
            Window::new(
                self.pass_when
                    .as_ref()
                    .map(|condition| condition.build(&context.enrichment_tables))
                    .transpose()?,
                self.flush_when.build(&context.enrichment_tables)?,
                self.events_before.unwrap_or(100),
                self.events_after.unwrap_or(0),
            )
            .unwrap(),
        ))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log)
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        // The event is not modified, so the definition is passed through as-is
        vec![TransformOutput::new(
            DataType::Log,
            clone_input_definitions(input_definitions),
        )]
    }
}
