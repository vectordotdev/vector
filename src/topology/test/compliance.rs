use std::pin::Pin;

use async_trait::async_trait;
use futures_util::{future::ready, Stream};
use vector_core::{transform::{FunctionTransform, OutputBuffer, TransformConfig, Transform, TransformContext, SyncTransform, TaskTransform, TransformOutputsBuf}, event::Event, schema::Definition, config::{Output, Input, DataType}};

enum TransformType {
	Function,
	Sync,
	Task,
}

struct NoopTransformConfig {
	transform_type: TransformType,
}

#[async_trait]
impl TransformConfig for NoopTransformConfig {
    fn input(&self) -> Input {
        Input::all()
    }

    fn outputs(&self, _: &Definition) -> Vec<Output> {
        vec![Output::default(DataType::all())]
    }

    fn transform_type(&self) -> &'static str {
        "noop"
    }

	async fn build(&self, globals: &TransformContext) -> crate::Result<Transform> {
        match self.transform_type {
			TransformType::Function => Ok(Transform::Function(Box::new(NoopTransform))),
			TransformType::Sync => Ok(Transform::Synchronous(Box::new(NoopTransform))),
			TransformType::Task => Ok(Transform::Task(Box::new(NoopTransform))),
		}
    }
}

#[derive(Clone)]
struct NoopTransform;

impl FunctionTransform for NoopTransform {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        output.push(event);
    }
}

impl<T> TaskTransform<T> for NoopTransform
where
	T: EventContainer,
{
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn futures_util::Stream<Item = T> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = T> + Send>> {
        let mut inner = self;
        Box::pin(task)
    }
}

#[tokio::test]
async fn test_function_transform() {

}

#[tokio::test]
async fn test_sync_transform() {

}

#[tokio::test]
async fn test_task_transform() {

}
