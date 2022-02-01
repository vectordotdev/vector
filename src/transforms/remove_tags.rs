use serde::{Deserialize, Serialize};

use crate::{
    config::{
        DataType, GenerateConfig, Output, TransformConfig, TransformContext, TransformDescription,
    },
    event::Event,
    transforms::{FunctionTransform, OutputBuffer, Transform},
};

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RemoveTagsConfig {
    pub tags: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct RemoveTags {
    tags: Vec<String>,
}

inventory::submit! {
    TransformDescription::new::<RemoveTagsConfig>("remove_tags")
}

impl GenerateConfig for RemoveTagsConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self { tags: Vec::new() }).unwrap()
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "remove_tags")]
impl TransformConfig for RemoveTagsConfig {
    async fn build(&self, _context: &TransformContext) -> crate::Result<Transform> {
        Ok(Transform::function(RemoveTags::new(self.tags.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Metric
    }

    fn outputs(&self) -> Vec<Output> {
        vec![Output::default(DataType::Metric)]
    }

    fn transform_type(&self) -> &'static str {
        "remove_tags"
    }
}

impl RemoveTags {
    pub fn new(tags: Vec<String>) -> Self {
        RemoveTags { tags }
    }
}

impl FunctionTransform for RemoveTags {
    fn transform(&mut self, output: &mut OutputBuffer, mut event: Event) {
        let metric = event.as_mut_metric();

        for tag in &self.tags {
            metric.remove_tag(tag);
            if metric.tags().is_none() {
                break;
            }
        }

        output.push(event);
    }
}

#[cfg(test)]
mod tests {
    use vector_common::btreemap;

    use super::*;
    use crate::{
        event::metric::{Metric, MetricKind, MetricValue},
        transforms::test::transform_one,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemoveTagsConfig>();
    }

    #[test]
    fn remove_tags() {
        let metric = Metric::new(
            "foo",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        )
        .with_tags(Some(btreemap! {
            "env" => "production",
            "region" => "us-east-1",
            "host" => "127.0.0.1",
        }));
        let expected = metric
            .clone()
            .with_tags(Some(btreemap! {"env" => "production"}));

        let mut transform = RemoveTags::new(vec!["region".into(), "host".into()]);
        let metric = transform_one(&mut transform, metric.into())
            .unwrap()
            .into_metric();

        assert_eq!(metric, expected);
    }

    #[test]
    fn remove_all_tags() {
        let metric = Metric::new(
            "foo",
            MetricKind::Incremental,
            MetricValue::Counter { value: 10.0 },
        )
        .with_tags(Some(btreemap! {"env" => "production"}));
        let expected = metric.clone().with_tags(None);

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform_one(&mut transform, metric.into())
            .unwrap()
            .into_metric();

        assert_eq!(metric, expected);
    }

    #[test]
    fn remove_tags_from_none() {
        let metric = Metric::new(
            "foo",
            MetricKind::Incremental,
            MetricValue::Set {
                values: vec!["bar".into()].into_iter().collect(),
            },
        );
        let expected = metric.clone().with_tags(None);

        let mut transform = RemoveTags::new(vec!["env".into()]);
        let metric = transform_one(&mut transform, metric.into())
            .unwrap()
            .into_metric();

        assert_eq!(metric, expected);
    }
}
