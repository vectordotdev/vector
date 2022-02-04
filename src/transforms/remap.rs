use std::{
    fs::File,
    io::{self, Read},
    path::PathBuf,
};

use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use vector_common::TimeZone;
use vrl::{
    diagnostic::{Formatter, Note},
    prelude::{DiagnosticError, ExpressionError},
    Program, Runtime, Terminate,
};

#[cfg(feature = "vrl-vm")]
use std::sync::Arc;
#[cfg(feature = "vrl-vm")]
use vrl::Vm;

use crate::{
    config::{
        log_schema, ComponentKey, DataType, Output, TransformConfig, TransformContext,
        TransformDescription,
    },
    event::{Event, VrlTarget},
    internal_events::{RemapMappingAbort, RemapMappingError},
    transforms::{SyncTransform, Transform, TransformOutputsBuf},
    Result,
};

const DROPPED: &str = "dropped";

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: Option<String>,
    pub file: Option<PathBuf>,
    #[serde(default)]
    pub timezone: TimeZone,
    pub drop_on_error: bool,
    #[serde(default = "crate::serde::default_true")]
    pub drop_on_abort: bool,
    pub reroute_dropped: bool,
}

inventory::submit! {
    TransformDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[async_trait::async_trait]
#[typetag::serde(name = "remap")]
impl TransformConfig for RemapConfig {
    async fn build(&self, context: &TransformContext) -> Result<Transform> {
        let remap = Remap::new(self.clone(), context)?;
        Ok(Transform::synchronous(remap))
    }

    fn input_type(&self) -> DataType {
        DataType::Any
    }

    fn outputs(&self) -> Vec<Output> {
        if self.reroute_dropped {
            vec![
                Output::default(DataType::Any),
                Output::from((DROPPED, DataType::Any)),
            ]
        } else {
            vec![Output::default(DataType::Any)]
        }
    }

    fn transform_type(&self) -> &'static str {
        "remap"
    }

    fn enable_concurrency(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct Remap {
    component_key: Option<ComponentKey>,
    program: Program,
    runtime: Runtime,

    #[cfg(feature = "vrl-vm")]
    vm: Arc<Vm>,
    timezone: TimeZone,
    drop_on_error: bool,
    drop_on_abort: bool,
    reroute_dropped: bool,
}

impl Remap {
    pub fn new(config: RemapConfig, context: &TransformContext) -> crate::Result<Self> {
        let source = match (&config.source, &config.file) {
            (Some(source), None) => source.to_owned(),
            (None, Some(path)) => {
                let mut buffer = String::new();

                File::open(path)
                    .with_context(|_| FileOpenFailedSnafu { path })?
                    .read_to_string(&mut buffer)
                    .with_context(|_| FileReadFailedSnafu { path })?;

                buffer
            }
            _ => return Err(Box::new(BuildError::SourceAndOrFile)),
        };

        let mut functions = vrl_stdlib::all();
        functions.append(&mut enrichment::vrl_functions());
        functions.append(&mut vector_vrl_functions::vrl_functions());

        let program = vrl::compile(
            &source,
            &functions,
            Some(Box::new(context.enrichment_tables.clone())),
        )
        .map_err(|diagnostics| Formatter::new(&source, diagnostics).colored().to_string())?;

        let runtime = Runtime::default();

        #[cfg(feature = "vrl-vm")]
        let vm = Arc::new(runtime.compile(functions, &program)?);

        Ok(Remap {
            component_key: context.key.clone(),
            program,
            runtime,
            timezone: config.timezone,
            drop_on_error: config.drop_on_error,
            drop_on_abort: config.drop_on_abort,
            reroute_dropped: config.reroute_dropped,
            #[cfg(feature = "vrl-vm")]
            vm,
        })
    }

    #[cfg(test)]
    const fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    fn annotate_dropped(&self, event: &mut Event, reason: &str, error: ExpressionError) {
        match event {
            Event::Log(ref mut log) => {
                let message = error
                    .notes()
                    .iter()
                    .filter(|note| matches!(note, Note::UserErrorMessage(_)))
                    .last()
                    .map(|note| note.to_string())
                    .unwrap_or_else(|| error.to_string());
                log.insert(
                    log_schema().metadata_key(),
                    serde_json::json!({
                        "dropped": {
                            "reason": reason,
                            "message": message,
                            "component_id": self.component_key,
                            "component_type": "remap",
                            "component_kind": "transform",
                        }
                    }),
                );
            }
            Event::Metric(ref mut metric) => {
                let m = log_schema().metadata_key();
                metric.insert_tag(format!("{}.dropped.reason", m), reason.into());
                metric.insert_tag(
                    format!("{}.dropped.component_id", m),
                    self.component_key
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(String::new),
                );
                metric.insert_tag(format!("{}.dropped.component_type", m), "remap".into());
                metric.insert_tag(format!("{}.dropped.component_kind", m), "transform".into());
            }
        }
    }

    #[cfg(feature = "vrl-vm")]
    fn run_vrl(&mut self, target: &mut VrlTarget) -> std::result::Result<vrl::Value, Terminate> {
        self.runtime.run_vm(&self.vm, target, &self.timezone)
    }

    #[cfg(not(feature = "vrl-vm"))]
    fn run_vrl(&mut self, target: &mut VrlTarget) -> std::result::Result<vrl::Value, Terminate> {
        let result = self.runtime.resolve(target, &self.program, &self.timezone);
        self.runtime.clear();
        result
    }
}

impl Clone for Remap {
    fn clone(&self) -> Self {
        Self {
            component_key: self.component_key.clone(),
            program: self.program.clone(),
            runtime: Runtime::default(),
            timezone: self.timezone,
            drop_on_error: self.drop_on_error,
            drop_on_abort: self.drop_on_abort,
            reroute_dropped: self.reroute_dropped,
            #[cfg(feature = "vrl-vm")]
            vm: Arc::clone(&self.vm),
        }
    }
}

impl SyncTransform for Remap {
    fn transform(&mut self, event: Event, output: &mut TransformOutputsBuf) {
        // If a program can fail or abort at runtime and we know that we will still need to forward
        // the event in that case (either to the main output or `dropped`, depending on the
        // config), we need to clone the original event and keep it around, to allow us to discard
        // any mutations made to the event while the VRL program runs, before it failed or aborted.
        //
        // The `drop_on_{error, abort}` transform config allows operators to remove events from the
        // main output if they're failed or aborted, in which case we can skip the cloning, since
        // any mutations made by VRL will be ignored regardless. If they hav configured
        // `reroute_dropped`, however, we still need to do the clone to ensure that we can forward
        // the event to the `dropped` output.
        let forward_on_error = !self.drop_on_error || self.reroute_dropped;
        let forward_on_abort = !self.drop_on_abort || self.reroute_dropped;
        let original_event = if (self.program.can_fail() && forward_on_error)
            || (self.program.can_abort() && forward_on_abort)
        {
            Some(event.clone())
        } else {
            None
        };

        let mut target: VrlTarget = event.into();
        let result = self.run_vrl(&mut target);

        match result {
            Ok(_) => {
                for event in target.into_events() {
                    output.push(event)
                }
            }
            Err(Terminate::Abort(error)) => {
                emit!(&RemapMappingAbort {
                    event_dropped: self.drop_on_abort,
                });

                if !self.drop_on_abort {
                    output.push(original_event.expect("event will be set"))
                } else if self.reroute_dropped {
                    let mut event = original_event.expect("event will be set");
                    self.annotate_dropped(&mut event, "abort", error);
                    output.push_named(DROPPED, event)
                }
            }
            Err(Terminate::Error(error)) => {
                emit!(&RemapMappingError {
                    error: error.to_string(),
                    event_dropped: self.drop_on_error,
                });

                if !self.drop_on_error {
                    output.push(original_event.expect("event will be set"))
                } else if self.reroute_dropped {
                    let mut event = original_event.expect("event will be set");
                    self.annotate_dropped(&mut event, "error", error);
                    output.push_named(DROPPED, event)
                }
            }
        }
    }
}

#[derive(Debug, Snafu)]
pub enum BuildError {
    #[snafu(display("must provide exactly one of `source` or `file` configuration"))]
    SourceAndOrFile,

    #[snafu(display("Could not open vrl program {:?}: {}", path, source))]
    FileOpenFailed { path: PathBuf, source: io::Error },
    #[snafu(display("Could not read vrl program {:?}: {}", path, source))]
    FileReadFailed { path: PathBuf, source: io::Error },
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap};

    use indoc::{formatdoc, indoc};
    use vector_common::btreemap;

    use super::*;
    use crate::{
        config::{build_unit_tests, ConfigBuilder},
        event::{
            metric::{MetricKind, MetricValue},
            LogEvent, Metric, Value,
        },
        test_util::components::{init_test, COMPONENT_MULTIPLE_OUTPUTS_TESTS},
        transforms::OutputBuffer,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    #[test]
    fn config_missing_source_and_file() {
        let config = RemapConfig {
            source: None,
            file: None,
            ..Default::default()
        };

        let err = Remap::new(config, &Default::default())
            .unwrap_err()
            .to_string();
        assert_eq!(
            &err,
            "must provide exactly one of `source` or `file` configuration"
        )
    }

    #[test]
    fn config_both_source_and_file() {
        let config = RemapConfig {
            source: Some("".to_owned()),
            file: Some("".into()),
            ..Default::default()
        };

        let err = Remap::new(config, &Default::default())
            .unwrap_err()
            .to_string();
        assert_eq!(
            &err,
            "must provide exactly one of `source` or `file` configuration"
        )
    }

    fn get_field_string(event: &Event, field: &str) -> String {
        event.as_log().get(field).unwrap().to_string_lossy()
    }

    #[test]
    fn check_remap_doesnt_share_state_between_events() {
        let conf = RemapConfig {
            source: Some(".foo = .sentinel".to_string()),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();
        assert!(tform.runtime().is_empty());

        let event1 = {
            let mut event1 = LogEvent::from("event1");
            event1.insert("sentinel", "bar");
            Event::from(event1)
        };
        let metadata1 = event1.metadata().clone();
        let result1 = transform_one(&mut tform, event1).unwrap();
        assert_eq!(get_field_string(&result1, "message"), "event1");
        assert_eq!(get_field_string(&result1, "foo"), "bar");
        assert_eq!(result1.metadata(), &metadata1);
        assert!(tform.runtime().is_empty());

        let event2 = {
            let event2 = LogEvent::from("event2");
            Event::from(event2)
        };
        let metadata2 = event2.metadata().clone();
        let result2 = transform_one(&mut tform, event2).unwrap();
        assert_eq!(get_field_string(&result2, "message"), "event2");
        assert_eq!(result2.as_log().get("foo"), Some(&Value::Null));
        assert_eq!(result2.metadata(), &metadata2);
        assert!(tform.runtime().is_empty());
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
            source: Some(
                r#"  .foo = "bar"
  .bar = "baz"
  .copy = .copy_from
"#
                .to_string(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let result = transform_one(&mut tform, event).unwrap();
        assert_eq!(get_field_string(&result, "message"), "augment me");
        assert_eq!(get_field_string(&result, "copy_from"), "buz");
        assert_eq!(get_field_string(&result, "foo"), "bar");
        assert_eq!(get_field_string(&result, "bar"), "baz");
        assert_eq!(get_field_string(&result, "copy"), "buz");
        assert_eq!(result.metadata(), &metadata);
    }

    #[test]
    fn check_remap_emits_multiple() {
        let event = {
            let mut event = LogEvent::from("augment me");
            event.insert(
                "events",
                vec![btreemap!("message" => "foo"), btreemap!("message" => "bar")],
            );
            Event::from(event)
        };
        let metadata = event.metadata().clone();

        let conf = RemapConfig {
            source: Some(
                indoc! {r#"
                . = .events
            "#}
                .to_owned(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let out = collect_outputs(&mut tform, event);
        assert_eq!(2, out.primary.len());
        let mut result = out.primary.into_events();

        let r = result.next().unwrap();
        assert_eq!(get_field_string(&r, "message"), "foo");
        assert_eq!(r.metadata(), &metadata);
        let r = result.next().unwrap();
        assert_eq!(get_field_string(&r, "message"), "bar");
        assert_eq!(r.metadata(), &metadata);
    }

    #[test]
    fn check_remap_error() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

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
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        assert!(transform_one(&mut tform, event).is_none())
    }

    #[test]
    fn check_remap_error_infallible() {
        let event = {
            let mut event = Event::from("augment me");
            event.as_mut_log().insert("bar", "is a string");
            event
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                .foo = "foo"
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

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
            source: Some(formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let event = transform_one(&mut tform, event).unwrap();

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
            source: Some(formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#}),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: true,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        assert!(transform_one(&mut tform, event).is_none())
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
            source: Some(
                r#".tags.host = "zoobub"
                       .name = "zork"
                       .namespace = "zerk"
                       .kind = "incremental""#
                    .to_string(),
            ),
            file: None,
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &Default::default()).unwrap();

        let result = transform_one(&mut tform, metric).unwrap();
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
    fn check_remap_branching() {
        let happy = Event::try_from(serde_json::json!({"hello": "world"})).unwrap();
        let abort = Event::try_from(serde_json::json!({"hello": "goodbye"})).unwrap();
        let error = Event::try_from(serde_json::json!({"hello": 42})).unwrap();

        let happy_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("hello".into(), "world".into());
            Event::Metric(metric)
        };

        let abort_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("hello".into(), "goodbye".into());
            Event::Metric(metric)
        };

        let error_metric = {
            let mut metric = Metric::new(
                "counter",
                MetricKind::Absolute,
                MetricValue::Counter { value: 1.0 },
            );
            metric.insert_tag("not_hello".into(), "oops".into());
            Event::Metric(metric)
        };

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                if exists(.tags) {{
                    # metrics
                    .tags.foo = "bar"
                    if string!(.tags.hello) == "goodbye" {{
                      abort
                    }}
                }} else {{
                    # logs
                    .foo = "bar"
                    if string!(.hello) == "goodbye" {{
                      abort
                    }}
                }}
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: true,
            ..Default::default()
        };
        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output = transform_one_fallible(&mut tform, happy).unwrap();
        let log = output.as_log();
        assert_eq!(log["hello"], "world".into());
        assert_eq!(log["foo"], "bar".into());
        assert!(!log.contains("metadata"));

        let output = transform_one_fallible(&mut tform, abort).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], "goodbye".into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "abort",
                    "message": "aborted",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );

        let output = transform_one_fallible(&mut tform, error).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], 42.into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "error",
                    "message": "function call error for \"string\" at (160:175): expected \"string\", got \"integer\"",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );

        let output = transform_one_fallible(&mut tform, happy_metric).unwrap();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("hello".into(), "world".into());
                    tags.insert("foo".into(), "bar".into());
                    tags
                }))
            )
        );

        let output = transform_one_fallible(&mut tform, abort_metric).unwrap_err();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("hello".into(), "goodbye".into());
                    tags.insert("metadata.dropped.reason".into(), "abort".into());
                    tags.insert("metadata.dropped.component_id".into(), "remapper".into());
                    tags.insert("metadata.dropped.component_type".into(), "remap".into());
                    tags.insert("metadata.dropped.component_kind".into(), "transform".into());
                    tags
                }))
            )
        );

        let output = transform_one_fallible(&mut tform, error_metric).unwrap_err();
        pretty_assertions::assert_eq!(
            output,
            Event::Metric(
                Metric::new(
                    "counter",
                    MetricKind::Absolute,
                    MetricValue::Counter { value: 1.0 },
                )
                .with_tags(Some({
                    let mut tags = BTreeMap::new();
                    tags.insert("not_hello".into(), "oops".into());
                    tags.insert("metadata.dropped.reason".into(), "error".into());
                    tags.insert("metadata.dropped.component_id".into(), "remapper".into());
                    tags.insert("metadata.dropped.component_type".into(), "remap".into());
                    tags.insert("metadata.dropped.component_kind".into(), "transform".into());
                    tags
                }))
            )
        );
    }

    #[test]
    fn check_remap_branching_assert_with_message() {
        let error_trigger_assert_custom_message =
            Event::try_from(serde_json::json!({"hello": 42})).unwrap();
        let error_trigger_default_assert_message =
            Event::try_from(serde_json::json!({"hello": 0})).unwrap();
        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                assert_eq!(.hello, 0, "custom message here")
                assert_eq!(.hello, 1)
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: true,
            ..Default::default()
        };
        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output =
            transform_one_fallible(&mut tform, error_trigger_assert_custom_message).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], 42.into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "error",
                    "message": "custom message here",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );

        let output =
            transform_one_fallible(&mut tform, error_trigger_default_assert_message).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], 0.into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "error",
                    "message": "function call error for \"assert_eq\" at (45:66): assertion failed: 0 == 1",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );
    }

    #[test]
    fn check_remap_branching_abort_with_message() {
        let error = Event::try_from(serde_json::json!({"hello": 42})).unwrap();
        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                abort "custom message here"
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: true,
            ..Default::default()
        };
        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output = transform_one_fallible(&mut tform, error).unwrap_err();
        let log = output.as_log();
        assert_eq!(log["hello"], 42.into());
        assert!(!log.contains("foo"));
        assert_eq!(
            log["metadata"],
            serde_json::json!({
                "dropped": {
                    "reason": "abort",
                    "message": "custom message here",
                    "component_id": "remapper",
                    "component_type": "remap",
                    "component_kind": "transform",
                }
            })
            .try_into()
            .unwrap()
        );
    }

    #[test]
    fn check_remap_branching_disabled() {
        let happy = Event::try_from(serde_json::json!({"hello": "world"})).unwrap();
        let abort = Event::try_from(serde_json::json!({"hello": "goodbye"})).unwrap();
        let error = Event::try_from(serde_json::json!({"hello": 42})).unwrap();

        let conf = RemapConfig {
            source: Some(formatdoc! {r#"
                if exists(.tags) {{
                    # metrics
                    .tags.foo = "bar"
                    if string!(.tags.hello) == "goodbye" {{
                      abort
                    }}
                }} else {{
                    # logs
                    .foo = "bar"
                    if string!(.hello) == "goodbye" {{
                      abort
                    }}
                }}
            "#}),
            drop_on_error: true,
            drop_on_abort: true,
            reroute_dropped: false,
            ..Default::default()
        };

        assert_eq!(vec![Output::default(DataType::Any)], conf.outputs());

        let context = TransformContext {
            key: Some(ComponentKey::from("remapper")),
            ..Default::default()
        };
        let mut tform = Remap::new(conf, &context).unwrap();

        let output = transform_one_fallible(&mut tform, happy).unwrap();
        let log = output.as_log();
        assert_eq!(log["hello"], "world".into());
        assert_eq!(log["foo"], "bar".into());
        assert!(!log.contains("metadata"));

        let out = collect_outputs(&mut tform, abort);
        assert!(out.primary.is_empty());
        assert!(out.named[DROPPED].is_empty());

        let out = collect_outputs(&mut tform, error);
        assert!(out.primary.is_empty());
        assert!(out.named[DROPPED].is_empty());
    }

    #[tokio::test]
    async fn check_remap_branching_metrics_with_output() {
        init_test();

        let config: ConfigBuilder = toml::from_str(indoc! {r#"
            [transforms.foo]
            inputs = []
            type = "remap"
            drop_on_abort = true
            reroute_dropped = true
            source = "abort"

            [[tests]]
            name = "metric output"

            [tests.input]
                insert_at = "foo"
                value = "none"

            [[tests.outputs]]
                extract_from = "foo.dropped"
                [[tests.outputs.conditions]]
                type = "vrl"
                source = "true"
        "#})
        .unwrap();

        let mut tests = build_unit_tests(config).await.unwrap();
        assert!(tests.remove(0).run().await.errors.is_empty());
        // Check that metrics were emitted with output tag
        COMPONENT_MULTIPLE_OUTPUTS_TESTS.assert(&["output"]);
    }

    struct CollectedOuput {
        primary: OutputBuffer,
        named: HashMap<String, OutputBuffer>,
    }

    fn collect_outputs(ft: &mut dyn SyncTransform, event: Event) -> CollectedOuput {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![
                Output::default(DataType::Any),
                Output::from((DROPPED, DataType::Any)),
            ],
            1,
        );

        ft.transform(event, &mut outputs);

        CollectedOuput {
            primary: outputs.take_primary(),
            named: outputs.take_all_named(),
        }
    }

    fn transform_one(ft: &mut dyn SyncTransform, event: Event) -> Option<Event> {
        let mut out = collect_outputs(ft, event);
        assert_eq!(0, out.named.iter().map(|(_, v)| v.len()).sum::<usize>());
        assert!(out.primary.len() <= 1);
        out.primary.pop()
    }

    fn transform_one_fallible(
        ft: &mut dyn SyncTransform,
        event: Event,
    ) -> std::result::Result<Event, Event> {
        let mut outputs = TransformOutputsBuf::new_with_capacity(
            vec![
                Output::default(DataType::Any),
                Output::from((DROPPED, DataType::Any)),
            ],
            1,
        );

        ft.transform(event, &mut outputs);

        let mut buf = outputs.drain().collect::<Vec<_>>();
        let mut err_buf = outputs.drain_named(DROPPED).collect::<Vec<_>>();

        assert!(buf.len() < 2);
        assert!(err_buf.len() < 2);
        match (buf.pop(), err_buf.pop()) {
            (Some(good), None) => Ok(good),
            (None, Some(bad)) => Err(bad),
            (a, b) => panic!("expected output xor error output, got {:?} and {:?}", a, b),
        }
    }
}
