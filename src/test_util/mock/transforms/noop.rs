use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use vector_lib::config::LogNamespace;
use vector_lib::configurable::configurable_component;
use vector_lib::{
    config::{DataType, Input, TransformOutput},
    event::{Event, EventContainer},
    schema::Definition,
    transform::{FunctionTransform, OutputBuffer, TaskTransform, Transform},
};

use crate::config::{GenerateConfig, OutputId, TransformConfig, TransformContext};

use super::TransformType;

/// Configuration for the `test_noop` transform.
#[configurable_component(transform("test_noop", "Test (no-op)"))]
#[derive(Clone, Debug)]
pub struct NoopTransformConfig {
    #[configurable(derived)]
    transform_type: TransformType,
}

impl GenerateConfig for NoopTransformConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            transform_type: TransformType::Function,
        })
        .unwrap()
    }
}

#[async_trait]
#[typetag::serde(name = "test_noop")]
impl TransformConfig for NoopTransformConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(
        &self,
        _: vector_lib::enrichment::TableRegistry,
        _: vector_lib::vrl_cache::VrlCacheRegistry,
        definitions: &[(OutputId, Definition)],
        _: LogNamespace,
    ) -> Vec<TransformOutput> {
        vec![TransformOutput::new(
            DataType::all_bits(),
            definitions
                .iter()
                .map(|(output, definition)| (output.clone(), definition.clone()))
                .collect(),
        )]
    }

    async fn build(&self, _: &TransformContext) -> crate::Result<Transform> {
        match self.transform_type {
            TransformType::Function => Ok(Transform::Function(Box::new(NoopTransform))),
            TransformType::Synchronous => Ok(Transform::Synchronous(Box::new(NoopTransform))),
            TransformType::Task => Ok(Transform::Task(Box::new(NoopTransform))),
        }
    }
}

impl From<TransformType> for NoopTransformConfig {
    fn from(transform_type: TransformType) -> Self {
        Self { transform_type }
    }
}

#[derive(Clone)]
struct NoopTransform;

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        output.push(event);
    }
}

impl<T> TaskTransform<T> for NoopTransform
where
    T: EventContainer + 'static,
{
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn futures_util::Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>> {
        Box::pin(task)
    }
}
