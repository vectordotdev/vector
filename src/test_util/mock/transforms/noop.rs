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

    /// Optional per-event busy-spin duration, in milliseconds.
    ///
    /// Unlike `delay_ms` (which sleeps, consuming no CPU), this actively burns
    /// CPU cycles on the calling thread. Intended for tests that need
    /// deterministic, non-zero component CPU usage: OS-level CPU-time clocks
    /// (e.g. Windows' `GetThreadTimes`) only update at clock-tick granularity,
    /// so a passthrough transform's negligible real work can otherwise round
    /// down to exactly zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cpu_burn_ms: Option<u64>,

    /// When `true`, the transform reports `enable_concurrency() == true`, causing
    /// the topology builder to run it on the concurrent (multi-task) code path.
    ///
    /// Only meaningful for `Function` and `Synchronous` transform types.
    #[serde(default, skip_serializing_if = "vector_lib::serde::is_default")]
    enable_concurrency: bool,
}

impl GenerateConfig for NoopTransformConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(&Self {
            transform_type: TransformType::Function,
            delay_ms: None,
            cpu_burn_ms: None,
            enable_concurrency: false,
        })
        .unwrap()
    }
}

impl NoopTransformConfig {
    pub fn with_delay_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = Some(delay_ms);
        self
    }

    pub fn with_cpu_burn_ms(mut self, cpu_burn_ms: u64) -> Self {
        self.cpu_burn_ms = Some(cpu_burn_ms);
        self
    }

    pub fn with_concurrency(mut self) -> Self {
        self.enable_concurrency = true;
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
        let cpu_burn = self.cpu_burn_ms.map(Duration::from_millis);
        match self.transform_type {
            TransformType::Function => Ok(Transform::Function(Box::new(NoopTransform {
                delay,
                cpu_burn,
            }))),
            TransformType::Synchronous => Ok(Transform::Synchronous(Box::new(NoopTransform {
                delay,
                cpu_burn,
            }))),
            TransformType::Task => Ok(Transform::Task(Box::new(NoopTransform { delay, cpu_burn }))),
        }
    }

    fn enable_concurrency(&self) -> bool {
        self.enable_concurrency
    }
}

impl From<TransformType> for NoopTransformConfig {
    fn from(transform_type: TransformType) -> Self {
        Self {
            transform_type,
            delay_ms: None,
            cpu_burn_ms: None,
            enable_concurrency: false,
        }
    }
}

#[derive(Clone)]
struct NoopTransform {
    delay: Option<Duration>,
    cpu_burn: Option<Duration>,
}

/// Actively burns CPU on the calling thread for `duration`, unlike `sleep`
/// which yields the thread without consuming CPU time.
fn busy_spin(duration: Duration) {
    let start = std::time::Instant::now();
    let mut acc: u64 = 0;
    while start.elapsed() < duration {
        acc = std::hint::black_box(acc.wrapping_add(1));
    }
    std::hint::black_box(acc);
}

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut OutputBuffer, event: Event) {
        if let Some(delay) = self.delay {
            std::thread::sleep(delay);
        }
        if let Some(cpu_burn) = self.cpu_burn {
            busy_spin(cpu_burn);
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
        let delay = self.delay;
        let cpu_burn = self.cpu_burn;
        if delay.is_some() || cpu_burn.is_some() {
            Box::pin(task.then(move |item| async move {
                if let Some(cpu_burn) = cpu_burn {
                    busy_spin(cpu_burn);
                }
                if let Some(delay) = delay {
                    tokio::time::sleep(delay).await;
                }
                item
            }))
        } else {
            Box::pin(task)
        }
    }
}
