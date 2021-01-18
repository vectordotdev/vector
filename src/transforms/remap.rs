use crate::{
    config::{DataType, TransformConfig, TransformDescription},
    event::Event,
    internal_events::RemapMappingError,
    transforms::{FunctionTransform, Transform},
    Result,
};
use remap::{value, Program, Runtime, TypeConstraint, TypeDef};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: String,
    pub drop_on_err: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    async fn build(&self) -> Result<Transform> {
        Remap::new(self.clone()).map(Transform::function)
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn output_type(&self) -> DataType {
        DataType::Any
    }

    fn transform_type(&self) -> &'static str {
        "remap"
    }
}

#[derive(Debug, Clone)]
pub struct Remap {
    program: Program,
    drop_on_err: bool,
}

impl Remap {
    pub fn new(config: RemapConfig) -> crate::Result<Self> {
        let accepts = TypeConstraint {
            allow_any: true,
            type_def: TypeDef {
                fallible: true,
                kind: value::Kind::all(),
                ..Default::default()
            },
        };

        let (program, _) = Program::new(
            config.source.clone(),
            &remap_functions::all(),
            Some(accepts),
            false,
        )
        .map_err(|diagnostics| {
            remap::Formatter::new(&config.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Remap {
            program,
            drop_on_err: config.drop_on_err,
        })
    }
}

impl FunctionTransform for Remap {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let mut runtime = Runtime::default();
        let result = match event {
            Event::Log(ref mut event) => runtime.run(event, &self.program),
            Event::Metric(ref mut event) => runtime.run(event, &self.program),
        };

        if let Err(error) = result {
            emit!(RemapMappingError {
                error: error.to_string(),
                event_dropped: self.drop_on_err,
            });

            if self.drop_on_err {
                return;
            }
        }

        output.push(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue},
        Metric,
    };
    use std::collections::BTreeMap;

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    fn get_field_string(event: &Event, field: &str) -> String {
        event.as_log().get(field).unwrap().to_string_lossy()
    }

    #[test]
    fn check_remap_adds() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("copy_from", "buz");
            event
        };

        let conf = RemapConfig {
            source: r#"  .foo = "bar"
  .bar = "baz"
  .copy = .copy_from
"#
            .to_string(),
            drop_on_err: true,
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(event).unwrap();
        assert_eq!(get_field_string(&result, "message"), "augment me");
        assert_eq!(get_field_string(&result, "copy_from"), "buz");
        assert_eq!(get_field_string(&result, "foo"), "bar");
        assert_eq!(get_field_string(&result, "bar"), "baz");
        assert_eq!(get_field_string(&result, "copy"), "buz");
    }

    #[test]
    fn check_remap_metric() {
        let metric = Event::Metric(Metric::new(
            "counter".into(),
            None,
            None,
            None,
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ));

        let conf = RemapConfig {
            source: r#".tags.host = "zoobub"
                       .name = "zork"
                       .namespace = "zerk"
                       .kind = "incremental""#
                .to_string(),
            drop_on_err: true,
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(metric).unwrap();
        assert_eq!(
            result,
            Event::Metric(Metric::new(
                "zork".into(),
                Some("zerk".into()),
                None,
                Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("host".into(), "zoobub".into());
                    tags
                }),
                MetricKind::Incremental,
                MetricValue::Counter { value: 1.0 },
            ))
        );
    }
}
