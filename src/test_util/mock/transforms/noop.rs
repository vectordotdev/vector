use std::pin::Pin;

use async_trait::async_trait;
use futures_util::Stream;
use vector_config::configurable_component;
use vector_core::config::LogNamespace;
use vector_core::{
    config::{DataType, Input, Output},
    event::{Event, EventContainer},
    schema::Definition,
    transform::{FunctionTransform, OutputBuffer, TaskTransform, Transform},
};

use crate::config::{GenerateConfig, TransformConfig, TransformContext};

use super::TransformType;

/// Configuration for the `test_noop` transform.
#[configurable_component(transform("test_noop"))]
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
impl TransformConfig for NoopTransformConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &Definition, _: LogNamespace) -> Vec<Output> {
        vec![Output::default(DataType::all())]
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
