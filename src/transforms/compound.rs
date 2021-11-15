use crate::{
    config::{DataType, GenerateConfig, TransformConfig, TransformContext, TransformDescription},
    event::Event,
    internal_events::CompoundErrorEvents,
    topology::builder::filter_event_type,
    transforms::{TaskTransform, Transform},
};
use futures::{stream, Stream, StreamExt};
use serde::{self, Deserialize, Serialize};
use std::pin::Pin;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CompoundConfig {
    steps: Vec<Box<dyn TransformConfig>>,
}

inventory::submit! {
    TransformDescription::new::<CompoundConfig>("compound")
}

impl GenerateConfig for CompoundConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { steps: Vec::new() }).unwrap()
    }
}

impl CompoundConfig {
    fn consistent_types(&self) -> bool {
        let mut pairs = self.steps.windows(2).map(|items| match items {
            [a, b] => (a.output_type(), b.input_type()),
            _ => unreachable!(),
        });

        !pairs.any(|pair| {
            matches!(
                pair,
                (DataType::Log, DataType::Metric) | (DataType::Metric, DataType::Log)
            )
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "compound")]
impl TransformConfig for CompoundConfig {
    async fn build(&self, context: &TransformContext) -> crate::Result<Transform> {
        if !self.consistent_types() {
            Err("Inconsistent type in a compound transform".into())
        } else {
            Compound::new(self.clone(), context)
                .await
                .map(Transform::task)
        }
    }

    fn input_type(&self) -> DataType {
        self.steps
            .first()
            .map(|t| t.input_type())
            .unwrap_or(DataType::Any)
    }

    fn output_type(&self) -> DataType {
        self.steps
            .last()
            .map(|t| t.output_type())
            .unwrap_or(DataType::Any)
    }

    fn transform_type(&self) -> &'static str {
        "compound"
    }
}

pub struct Compound {
    transforms: Vec<(Transform, DataType)>,
}

impl Compound {
    pub async fn new(config: CompoundConfig, context: &TransformContext) -> crate::Result<Self> {
        let steps = &config.steps;
        let mut transforms = vec![];
        if !steps.is_empty() {
            for transform_config in steps.iter() {
                let transform = transform_config.build(context).await?;
                transforms.push((transform, transform_config.input_type()));
            }
            Ok(Self { transforms })
        } else {
            Err("must specify at least one transform".into())
        }
    }
}

impl TaskTransform for Compound {
    fn transform(
        self: Box<Self>,
        task: Pin<Box<dyn Stream<Item = Event> + Send>>,
    ) -> Pin<Box<dyn Stream<Item = Event> + Send>>
    where
        Self: 'static,
    {
        let mut task = task;
        let mut idx = 0;
        for t in self.transforms {
            task = filter_event_type(Box::pin(task), t.1);
            match t.0 {
                Transform::Task(t) => {
                    task = t.transform(task);
                }
                Transform::Function(mut t) => {
                    task = Box::pin(task.flat_map(move |v| {
                        let mut output = Vec::<Event>::new();
                        error_span!(
                            "compound_inner_transform",
                            component_id = %idx,
                            component_name = %idx,
                        )
                        .in_scope(|| {
                            t.transform(&mut output, v);
                        });
                        stream::iter(output)
                    }));
                }
                Transform::FallibleFunction(mut t) => {
                    task = Box::pin(task.flat_map(move |v| {
                        let mut output = Vec::<Event>::new();
                        let mut errors = Vec::<Event>::new();
                        error_span!(
                            "compound_inner_transform",
                            component_id = %idx,
                            component_name = %idx,
                        )
                        .in_scope(|| {
                            t.transform(&mut output, &mut errors, v);
                        });
                        emit!(&CompoundErrorEvents {
                            count: errors.len(),
                        });
                        stream::iter(output)
                    }));
                }
            }
            idx += 1;
        }
        task
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<super::CompoundConfig>();
    }
}
