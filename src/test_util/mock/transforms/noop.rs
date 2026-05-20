use std::{pin::Pin, time::Duration};

use async_trait::async_trait;
use futures_util::{Stream, StreamExt as _};
use vector_lib::{
    config::{DataType, Input, TransformOutput},
    configurable::configurable_component,
    event::{Event, EventContainer},
    schema::Definition,
    transform::{FunctionTransform, OutputBuffer, TaskTransform, Transform},
};

use super::TransformType;
use crate::config::{GenerateConfig, OutputId, TransformConfig, TransformContext};

/// Configuration for the `test_noop` transform.
#[configurable_component(transform("test_noop", "Test (no-op)"))]
#[derive(Clone, Debug)]
pub struct NoopTransformConfig {
    #[configurable(derived)]
    transform_type: TransformType,

    /// Optional per-event/array delay, in milliseconds.
    ///
    /// This is intended for tests that need deterministic, non-zero component latency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    delay_ms: Option<u64>,
}

impl GenerateConfig for NoopTransformConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            transform_type: TransformType::Function,
            delay_ms: None,
        })
        .unwrap()
    }
}

impl NoopTransformConfig {
    pub fn with_delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = Some(delay_ms);
        self
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
        _: &TransformContext,
        definitions: &[(OutputId, Definition)],
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
        let delay = self.delay_ms.map(Duration::from_millis);
        match self.transform_type {
            TransformType::Function => Ok(Transform::Function(Box::new(NoopTransform { delay }))),
            TransformType::Synchronous => {
                Ok(Transform::Synchronous(Box::new(NoopTransform { delay })))
            }
            TransformType::Task => Ok(Transform::Task(Box::new(NoopTransform { delay }))),
        }
    }
}

impl From<TransformType> for NoopTransformConfig {
    fn from(transform_type: TransformType) -> Self {
        Self {
            transform_type,
            delay_ms: None,
        }
    }
}

#[derive(Clone)]
struct NoopTransform {
    delay: Option<Duration>,
}

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        output.push(event);
    }
}

impl<T> TaskTransform<T> for NoopTransform
where
    T: EventContainer + Send + 'static,
{
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn futures_util::Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>> {
        if let Some(delay) = self.delay {
            Box::pin(task.then(move |item| async move {
                tokio::time::sleep(delay).await;
                item
            }))
        } else {
            Box::pin(task)
        }
    }
}
