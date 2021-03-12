use crate::{
    config::{DataType, GlobalOptions, TransformConfig, TransformDescription},
    event::Event,
    internal_events::{RemapMappingAbort, RemapMappingError},
    transforms::{FunctionTransform, Transform},
    Result,
};
use serde::{Deserialize, Serialize};
use vrl::diagnostic::Formatter;
use vrl::{Program, Runtime, Terminate};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: String,
    pub drop_on_error: bool,
    #[serde(default = "crate::serde::default_true")]
    pub drop_on_abort: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    async fn build(&self, _globals: &GlobalOptions) -> Result<Transform> {
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
    drop_on_error: bool,
    drop_on_abort: bool,
}

impl Remap {
    pub fn new(config: RemapConfig) -> crate::Result<Self> {
        let program = vrl::compile(&config.source, &vrl_stdlib::all()).map_err(|diagnostics| {
            Formatter::new(&config.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Remap {
            program,
            drop_on_error: config.drop_on_error,
            drop_on_abort: config.drop_on_abort,
        })
    }
}

impl FunctionTransform for Remap {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        let original_event =
            if !(self.drop_on_error || self.drop_on_abort) && self.program.is_fallible() {
                // We need to clone the original event, since it might be mutated by
                // the program before it aborts, while we want to return the
                // unmodified event when an error occurs.
                Some(event.clone())
            } else {
                None
            };

        let mut runtime = Runtime::default();

        let result = match event {
            Event::Log(ref mut event) => runtime.resolve(event, &self.program),
            Event::Metric(ref mut event) => runtime.resolve(event, &self.program),
        };

        match result {
            Ok(_) => output.push(event),
            Err(Terminate::Abort) => {
                emit!(RemapMappingAbort {
                    event_dropped: self.drop_on_abort,
                });

                if self.drop_on_abort {
                    return;
                }

                output.push(original_event.unwrap_or(event))
            }
            Err(Terminate::Error(error)) => {
                emit!(RemapMappingError {
                    error,
                    event_dropped: self.drop_on_error,
                });

                if self.drop_on_error {
                    return;
                }

                output.push(original_event.unwrap_or(event))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue},
        Metric, Value,
    };
    use indoc::formatdoc;
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
            drop_on_error: true,
            drop_on_abort: false,
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
    fn check_remap_error() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#},
            drop_on_error: false,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

        let event = tform.transform_one(event).unwrap();

        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));

        assert!(event.as_log().get("foo").is_none());
        assert!(event.as_log().get("baz").is_none());
    }

    #[test]
    fn check_remap_error_infallible() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: formatdoc! {r#"
                .foo = "foo"
                .baz = 12
            "#},
            drop_on_error: false,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

        let event = tform.transform_one(event).unwrap();

        assert_eq!(event.as_log().get("foo"), Some(&Value::from("foo")));
        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert_eq!(event.as_log().get("baz"), Some(&Value::from(12)));
    }

    #[test]
    fn check_remap_metric() {
        let metric = Event::Metric(Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ));

        let conf = RemapConfig {
            source: r#".tags.host = "zoobub"
                       .name = "zork"
                       .namespace = "zerk"
                       .kind = "incremental""#
                .to_string(),
            drop_on_error: true,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(metric).unwrap();
        assert_eq!(
            result,
            Event::Metric(
                Metric::new(
                    "zork",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_namespace(Some("zerk"))
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("host".into(), "zoobub".into());
                    tags
                }))
            )
        );
    }
}
