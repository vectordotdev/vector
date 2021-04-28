use crate::{
    config::{DataType, GlobalOptions, TransformConfig, TransformDescription},
    event::{self, Event, LogEvent},
    internal_events::{RemapMappingAbort, RemapMappingError},
    transforms::{FunctionTransform, Transform},
    Result,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vrl::diagnostic::Formatter;
use vrl::{Program, Runtime, Terminate, Value};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: String,
    pub drop_on_error: bool,
    #[serde(default = "crate::serde::default_true")]
    pub drop_on_abort: bool,
    pub emit_multiple: bool,
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
    emit_multiple: bool,
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
            emit_multiple: config.emit_multiple,
        })
    }
}

impl FunctionTransform for Remap {
    fn transform(&mut self, output: &mut Vec<Event>, mut event: Event) {
        // If a program can fail or abort at runtime, we need to clone the
        // original event and keep it around, to allow us to discard any
        // mutations made to the event while the VRL program runs, before it
        // failed or aborted.
        //
        // The `drop_on_{error, abort}` transform config allows operators to
        // ignore events if their failed/aborted, in which case we can skip the
        // cloning, since any mutations made by VRL will be ignored regardless.
        #[allow(clippy::if_same_then_else)]
        let original_event = if !self.drop_on_error && self.program.can_fail() {
            Some(event.clone())
        } else if !self.drop_on_abort && self.program.can_abort() {
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
            Ok(value) => match (self.emit_multiple, value, &event) {
                (false, _, _) => output.push(event),
                (true, Value::Array(array), Event::Log(log)) => {
                    let objects = array
                        .into_iter()
                        .map(|value| match value {
                            Value::Object(object) => Ok(object
                                .into_iter()
                                .map(|(key, value)| (key, Into::<event::Value>::into(value)))
                                .collect::<BTreeMap<_, _>>()),
                            _ => Err(format!(
                                "target must be a valid object, got {}: {}",
                                value.kind(),
                                value
                            )),
                        })
                        .collect::<std::result::Result<Vec<_>, _>>();

                    let objects = match objects {
                        Ok(objects) => objects,
                        Err(error) => {
                            if !self.drop_on_error {
                                output.push(original_event.unwrap_or(event))
                            }

                            return emit!(RemapMappingError {
                                error,
                                event_dropped: self.drop_on_error
                            });
                        }
                    };

                    let metadata = log.metadata();

                    for object in objects {
                        let event = LogEvent::new(object, metadata.clone());
                        output.push(event.into());
                    }
                }
                (true, Value::Array(_), Event::Metric(_)) => todo!(),
                (true, value, _) => {
                    if !self.drop_on_error {
                        output.push(original_event.unwrap_or(event))
                    }

                    return emit!(RemapMappingError {
                        error: format!(
                            "target must be a valid array, got {}: {}",
                            value.kind(),
                            value
                        ),
                        event_dropped: self.drop_on_error
                    });
                }
            },
            Err(Terminate::Abort) => {
                if !self.drop_on_abort {
                    output.push(original_event.unwrap_or(event))
                }

                emit!(RemapMappingAbort {
                    event_dropped: self.drop_on_abort,
                });
            }
            Err(Terminate::Error(error)) => {
                if !self.drop_on_error {
                    output.push(original_event.unwrap_or(event))
                }

                emit!(RemapMappingError {
                    error,
                    event_dropped: self.drop_on_error,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{
        metric::{MetricKind, MetricValue},
        EventMetadata, LogEvent, Metric, Value,
    };
    use indoc::indoc;
    use shared::btreemap;
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
            let mut event = LogEvent::from("augment me");
            event.insert("copy_from", "buz");
            Event::from(event)
        };
        let metadata = event.metadata().clone();

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "bar"
                .bar = "baz"
                .copy = .copy_from
            "#}
            .to_owned(),
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(event).unwrap();
        assert_eq!(get_field_string(&result, "message"), "augment me");
        assert_eq!(get_field_string(&result, "copy_from"), "buz");
        assert_eq!(get_field_string(&result, "foo"), "bar");
        assert_eq!(get_field_string(&result, "bar"), "baz");
        assert_eq!(get_field_string(&result, "copy"), "buz");
        assert_eq!(result.metadata(), &metadata);
    }

    #[test]
    fn check_remap_error() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}
            .to_owned(),
            drop_on_error: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        let event = tform.transform_one(event).unwrap();

        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert!(event.as_log().get("foo").is_none());
        assert!(event.as_log().get("baz").is_none());
    }

    #[test]
    fn check_remap_error_drop() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}
            .to_owned(),
            drop_on_error: true,
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        assert!(tform.transform_one(event).is_none())
    }

    #[test]
    fn check_remap_error_infallible() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "foo"
                .baz = 12
            "#}
            .to_owned(),
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        let event = tform.transform_one(event).unwrap();

        assert_eq!(event.as_log().get("foo"), Some(&Value::from("foo")));
        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert_eq!(event.as_log().get("baz"), Some(&Value::from(12)));
    }

    #[test]
    fn check_remap_abort() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}
            .to_owned(),
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        let event = tform.transform_one(event).unwrap();

        assert_eq!(event.as_log().get("bar"), Some(&Value::from("is a string")));
        assert!(event.as_log().get("foo").is_none());
        assert!(event.as_log().get("baz").is_none());
    }

    #[test]
    fn check_remap_abort_drop() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: indoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}
            .to_owned(),
            drop_on_abort: true,
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        assert!(tform.transform_one(event).is_none())
    }

    #[test]
    fn check_remap_metric() {
        let metric = Event::Metric(Metric::new(
            "counter",
            MetricKind::Absolute,
            MetricValue::Counter { value: 1.0 },
        ));
        let metadata = metric.metadata().clone();

        let conf = RemapConfig {
            source: indoc! {r#"
                .tags.host = "zoobub"
                .name = "zork"
                .namespace = "zerk"
                .kind = "incremental"
            "# }
            .to_owned(),
            ..Default::default()
        };
        let mut tform = Remap::new(conf).unwrap();

        let result = tform.transform_one(metric).unwrap();
        assert_eq!(
            result,
            Event::Metric(
                Metric::new_with_metadata(
                    "zork",
                    MetricKind::Incremental,
                    MetricValue::Counter { value: 1.0 },
                    metadata,
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

    #[test]
    fn check_remap_multiple_logs() {
        let metadata: EventMetadata = Default::default();
        let event: Event = LogEvent::new(
            btreemap! {
                "foo" => btreemap! {
                    "source" => "foo",
                },
                "bar" => btreemap! {
                    "source" => "bar",
                },
            },
            metadata.clone(),
        )
        .into();

        let conf = RemapConfig {
            source: indoc! {"
                . = [.foo, .bar]
            "}
            .to_owned(),
            emit_multiple: true,
            ..Default::default()
        };

        let mut transform = Remap::new(conf).unwrap();

        let mut output = Vec::new();
        transform.transform(&mut output, event);

        assert_eq!(output.len(), 2);
        assert_eq!(
            output[0].as_log().as_map(),
            &btreemap! {
                "source" => "foo"
            }
        );
        assert_eq!(output[0].metadata(), &metadata);
        assert_eq!(
            output[1].as_log().as_map(),
            &btreemap! {
                "source" => "bar"
            }
        );
        assert_eq!(output[1].metadata(), &metadata);
    }

    #[test]
    fn check_remap_multiple_logs_error_no_array() {
        let metadata: EventMetadata = Default::default();
        let event: Event = LogEvent::new(btreemap! {}, metadata.clone()).into();

        let conf = RemapConfig {
            source: indoc! {r#"
                . = "not an array"
            "#}
            .to_owned(),
            emit_multiple: true,
            drop_on_error: false,
            ..Default::default()
        };

        let mut transform = Remap::new(conf).unwrap();

        let mut output = Vec::new();
        transform.transform(&mut output, event.clone());

        assert_eq!(output, [event.clone()]);
    }

    #[test]
    fn check_remap_multiple_logs_error_no_object() {
        let metadata: EventMetadata = Default::default();
        let event: Event = LogEvent::new(btreemap! {}, metadata.clone()).into();

        let conf = RemapConfig {
            source: indoc! {r#"
                . = ["not an object"]
            "#}
            .to_owned(),
            emit_multiple: true,
            drop_on_error: false,
            ..Default::default()
        };

        let mut transform = Remap::new(conf).unwrap();

        let mut output = Vec::new();
        transform.transform(&mut output, event.clone());

        assert_eq!(output, [event.clone()]);
    }

    #[test]
    fn check_remap_multiple_logs_error_no_array_drop() {
        let metadata: EventMetadata = Default::default();
        let event: Event = LogEvent::new(btreemap! {}, metadata.clone()).into();

        let conf = RemapConfig {
            source: indoc! {r#"
                . = "not an array"
            "#}
            .to_owned(),
            emit_multiple: true,
            drop_on_error: true,
            ..Default::default()
        };

        let mut transform = Remap::new(conf).unwrap();

        let mut output = Vec::new();
        transform.transform(&mut output, event);

        assert_eq!(output, []);
    }

    #[test]
    fn check_remap_multiple_logs_error_no_object_drop() {
        let metadata: EventMetadata = Default::default();
        let event: Event = LogEvent::new(btreemap! {}, metadata.clone()).into();

        let conf = RemapConfig {
            source: indoc! {r#"
                . = ["not an object"]
            "#}
            .to_owned(),
            emit_multiple: true,
            drop_on_error: true,
            ..Default::default()
        };

        let mut transform = Remap::new(conf).unwrap();

        let mut output = Vec::new();
        transform.transform(&mut output, event);

        assert_eq!(output, []);
    }
}
