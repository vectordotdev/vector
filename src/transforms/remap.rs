use std::sync::Arc;

use crate::{
    config::{DataType, TransformConfig, TransformContext, TransformDescription},
    enrichment_tables::EnrichmentTable,
    event::{Event, VrlTarget},
    internal_events::{RemapMappingAbort, RemapMappingError},
    transforms::{FunctionTransform, Transform},
    Result,
};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use shared::TimeZone;
use std::collections::HashMap;
use vrl::diagnostic::Formatter;
use vrl::{Program, Runtime, Terminate};

#[derive(Deserialize, Serialize, Debug, Clone, Derivative)]
#[serde(deny_unknown_fields, default)]
#[derivative(Default)]
pub struct RemapConfig {
    pub source: String,
    #[serde(default)]
    pub timezone: TimeZone,
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
    async fn build(&self, context: &TransformContext) -> Result<Transform> {
        Remap::new(self.clone(), context.enrichment_tables.clone()).map(Transform::function)
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

#[derive(Clone)]
pub struct Remap {
    program: Program,
    timezone: TimeZone,
    drop_on_error: bool,
    drop_on_abort: bool,
    enrichment_tables: Arc<ArcSwap<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>,
}

lazy_static::lazy_static! {
    static ref MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

impl Remap {
    pub fn new(
        config: RemapConfig,
        enrichment_tables: Arc<ArcSwap<HashMap<String, Box<dyn EnrichmentTable + Send + Sync>>>>,
    ) -> crate::Result<Self> {
        // Add a dummy index to test it works. This is is not final code, ultimately the index
        // creation will occur within VRL as the remap code is compiled.
        //
        // Ensure we don't have multiple threads running this code at the same time, since the
        // enrichment_tables is essentially global data, whilst we are adding the index we are
        // swapping that data out of the structure. If there were two Remaps being compiled at the
        // same time is separate threads it could result in one compilation accessing the
        // empty enrichment tables, and thus compiling incorrectly.
        let lock = MUTEX.lock().unwrap();

        let mut tables = enrichment_tables.swap(Default::default());
        match Arc::get_mut(&mut tables).unwrap().get_mut("file") {
            None => (),
            Some(table) => table.add_index(vec!["field1"]),
        }
        enrichment_tables.swap(tables);

        drop(lock);

        let program = vrl::compile(&config.source, &vrl_stdlib::all()).map_err(|diagnostics| {
            Formatter::new(&config.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Remap {
            program,
            timezone: config.timezone,
            drop_on_error: config.drop_on_error,
            drop_on_abort: config.drop_on_abort,
            enrichment_tables,
        })
    }
}

impl FunctionTransform for Remap {
    fn transform(&mut self, output: &mut Vec<Event>, event: Event) {
        let tables = self.enrichment_tables.load();
        for (key, value) in tables.iter() {
            trace!(
                "Testing we have {} {:?} {:?}",
                key,
                value.find_table_row(std::collections::BTreeMap::new()),
                value
            );
        }

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

        let mut target: VrlTarget = event.into();

        let mut runtime = Runtime::default();

        let result = runtime.resolve(&mut target, &self.program, &self.timezone);

        match result {
            Ok(_) => {
                for event in target.into_events() {
                    output.push(event)
                }
            }
            Err(Terminate::Abort(_)) => {
                emit!(RemapMappingAbort {
                    event_dropped: self.drop_on_abort,
                });

                if !self.drop_on_abort {
                    output.push(original_event.expect("event will be set"))
                }
            }
            Err(Terminate::Error(error)) => {
                emit!(RemapMappingError {
                    error: error.to_string(),
                    event_dropped: self.drop_on_error,
                });

                if !self.drop_on_error {
                    output.push(original_event.expect("event will be set"))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        event::{
            metric::{MetricKind, MetricValue},
            LogEvent, Metric, Value,
        },
        transforms::test::transform_one,
    };
    use indoc::{formatdoc, indoc};
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
            source: r#"  .foo = "bar"
  .bar = "baz"
  .copy = .copy_from
"#
            .to_string(),
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: indoc! {r#"
                . = .events
            "#}
            .to_owned(),
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

        let mut result = vec![];
        tform.transform(&mut result, event);

        assert_eq!(get_field_string(&result[0], "message"), "foo");
        assert_eq!(get_field_string(&result[1], "message"), "bar");
        assert_eq!(result[0].metadata(), &metadata);
        assert_eq!(result[1].metadata(), &metadata);
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
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: formatdoc! {r#"
                .foo = "foo"
                .not_an_int = int!(.bar)
                .baz = 12
            "#},
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: formatdoc! {r#"
                .foo = "foo"
                .baz = 12
            "#},
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#},
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: formatdoc! {r#"
                .foo = "foo"
                abort
                .baz = 12
            "#},
            timezone: TimeZone::default(),
            drop_on_error: false,
            drop_on_abort: true,
        };
        let mut tform = Remap::new(conf).unwrap();

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
            source: r#".tags.host = "zoobub"
                       .name = "zork"
                       .namespace = "zerk"
                       .kind = "incremental""#
                .to_string(),
            timezone: TimeZone::default(),
            drop_on_error: true,
            drop_on_abort: false,
        };
        let mut tform = Remap::new(conf).unwrap();

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
}
