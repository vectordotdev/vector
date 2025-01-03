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

use super::transform::Gate;


/// Configuration for the `gate` transform.
#[configurable_component(transform(
"gate",
"Open or close an event stream based on supplied criteria"
))]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct GateConfig {
    /// A logical condition used to pass events through without gating.
    pub pass_when: Option<AnyCondition>,

    /// A logical condition used to open the gate.
    pub open_when: Option<AnyCondition>,

    /// A logical condition used to close the gate.
    pub close_when: Option<AnyCondition>,

    /// Maximum number of events to keep in the buffer.
    pub max_events: Option<usize>,

    /// Automatically close the gate after the buffer has been flushed.
    pub auto_close: Option<bool>,

    /// Keep the gate open for additional number of events after the buffer has been flushed.
    pub tail_events: Option<usize>,
}

impl GenerateConfig for GateConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            pass_when: None::<AnyCondition>,
            open_when: None::<AnyCondition>,
            close_when: None::<AnyCondition>,
            max_events: None::<usize>,
            auto_close: None::<bool>,
            tail_events: None::<usize>,
        }).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "gate")]
impl TransformConfig for GateConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(Gate::new(
            self.pass_when
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
            self.open_when
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
            self.close_when
                .as_ref()
                .map(|condition| condition.build(&context.enrichment_tables))
                .transpose()?,
            self.max_events.unwrap_or(100),
            self.auto_close.unwrap_or(true),
            self.tail_events.unwrap_or(10),
        ).unwrap()))
    }

    fn input(&self) -> Input {
        Input::new(DataType::Log | DataType::Trace)
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        input_definitions: &[(OutputId, schema::Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        // The event is not modified, so the definition is passed through as-is
        vec![TransformOutput::new(
            DataType::Log | DataType::Trace,
            clone_input_definitions(input_definitions),
        )]
    }
}
