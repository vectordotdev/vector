use crate::{
    conditions::{Condition, ConditionConfig, ConditionDescription},
    emit,
    internal_events::RemapConditionExecutionError,
    Event,
};
use remap::{value, Program, Runtime, TypeConstraint, TypeDef, Value};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct RemapConfig {
    source: String,
}

inventory::submit! {
    ConditionDescription::new::<RemapConfig>("remap")
}

impl_generate_config_from_default!(RemapConfig);

#[typetag::serde(name = "remap")]
impl ConditionConfig for RemapConfig {
    fn build(&self) -> crate::Result<Box<dyn Condition>> {
        let constraint = TypeConstraint {
            allow_any: false,
            type_def: TypeDef {
                fallible: true,
                kind: value::Kind::Boolean,
                ..Default::default()
            },
        };

        // Filter out functions that directly mutate the event.
        //
        // TODO(jean): expose this as a method on the `Function` trait, so we
        // don't need to do this manually.
        let functions = remap_functions::all()
            .into_iter()
            .filter(|f| f.identifier() != "del")
            .filter(|f| f.identifier() != "only_fields")
            .collect::<Vec<_>>();

        let (program, _) = Program::new(self.source.clone(), &functions, Some(constraint), false)
            .map_err(|diagnostics| {
            remap::Formatter::new(&self.source, diagnostics)
                .colored()
                .to_string()
        })?;

        Ok(Box::new(Remap { program }))
    }
}

//------------------------------------------------------------------------------

#[derive(Clone)]
pub struct Remap {
    program: Program,
}

impl Remap {
    fn run(&self, event: &Event) -> remap::RuntimeResult {
        // TODO(jean): This clone exists until remap-lang has an "immutable"
        // mode.
        //
        // For now, mutability in reduce "remap ends-when conditions" is
        // allowed, but it won't mutate the original event, since we cloned it
        // here.
        //
        // Having first-class immutability support in the language allows for
        // more performance (one less clone), and boot-time errors when a
        // program wants to mutate its events.
        //
        // see: https://github.com/timberio/vector/issues/4744
        match event {
            Event::Log(event) => Runtime::default().run(&mut event.clone(), &self.program),
            Event::Metric(event) => Runtime::default().run(&mut event.clone(), &self.program),
        }
    }
}

impl Condition for Remap {
    fn check(&self, event: &Event) -> bool {
        self.run(&event)
            .map(|value| match value {
                Value::Boolean(boolean) => boolean,
                _ => unreachable!("boolean type constraint set"),
            })
            .unwrap_or_else(|_| {
                emit!(RemapConditionExecutionError);
                false
            })
    }

    fn check_with_context(&self, event: &Event) -> Result<(), String> {
        let value = self
            .run(event)
            .map_err(|err| format!("source execution failed: {:#}", err))?;

        match value {
            Value::Boolean(v) if v => Ok(()),
            Value::Boolean(v) if !v => Err("source execution resolved to false".into()),
            _ => unreachable!("boolean type constraint set"),
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use super::*;
    use crate::{event::Metric, event::MetricKind, event::MetricValue, log_event};

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<RemapConfig>();
    }

    #[test]
    fn check_remap() {
        let checks = vec![
            (
                log_event![],   // event
                "true == true", // source
                Ok(()),         // build result
                Ok(()),         // check result
            ),
            (
                log_event!["foo" => true, "bar" => false],
                "to_bool(.bar || .foo) ?? false",
                Ok(()),
                Ok(()),
            ),
            (
                log_event![],
                "true == false",
                Ok(()),
                Err("source execution resolved to false"),
            ),
            (
                log_event![],
                "null",
                Err("error: unexpected return value\n  ┌─ :1:1\n  │\n1 │ null\n  │ ^^^^\n  │ │\n  │ got: null\n  │ expected: boolean\n  │\n  = see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
                Ok(()),
            ),
            (
                log_event!["foo" => "string"],
                ".foo",
                Err("error: unexpected return value\n  ┌─ :1:1\n  │\n1 │ .foo\n  │ ^^^^\n  │ │\n  │ got: any\n  │ expected: boolean\n  │\n  = see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
                Ok(()),
            ),
            (
                log_event![],
                ".",
                Err("error: unexpected return value\n  ┌─ :1:1\n  │\n1 │ .\n  │ ^\n  │ │\n  │ got: any\n  │ expected: boolean\n  │\n  = see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
                Ok(()),
            ),
            (
                Event::Metric(Metric {
                    name: "zork".into(),
                    namespace: Some("zerk".into()),
                    timestamp: None,
                    tags: Some({
                        let mut tags = BTreeMap::new();
                        tags.insert("host".into(), "zoobub".into());
                        tags
                    }),
                    kind: MetricKind::Incremental,
                    value: MetricValue::Counter { value: 1.0 },
                }),
                r#".name == "zork" && .tags.host == "zoobub" && .kind == "incremental""#,
                Ok(()),
                Ok(()),
            )
        ];

        for (event, source, build, check) in checks {
            let source = source.to_owned();
            let config = RemapConfig { source };

            assert_eq!(
                config.build().map(|_| ()).map_err(|e| e.to_string()),
                build.map_err(|e| e.to_string())
            );

            if let Ok(cond) = config.build() {
                assert_eq!(
                    cond.check_with_context(&event),
                    check.map_err(|e| e.to_string())
                );
            }
        }
    }
}
