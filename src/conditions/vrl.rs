use vector_lib::configurable::configurable_component;
use vector_lib::{compile_vrl, emit, TimeZone};
use vrl::compiler::runtime::{Runtime, RuntimeResult, Terminate};
use vrl::compiler::{CompilationResult, CompileConfig, Program, TypeState, VrlRuntime};
use vrl::diagnostic::Formatter;
use vrl::value::Value;

use crate::config::LogNamespace;
use crate::event::TargetEvents;
use crate::{
    conditions::{Condition, Conditional, ConditionalConfig},
    event::{Event, VrlTarget},
    internal_events::VrlConditionExecutionError,
};

/// A condition that uses the [Vector Remap Language](https://vector.dev/docs/reference/vrl) (VRL) [boolean expression](https://vector.dev/docs/reference/vrl#boolean-expressions) against an event.
#[configurable_component]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VrlConfig {
    /// The VRL boolean expression.
    pub(crate) source: String,

    #[configurable(derived, metadata(docs::hidden))]
    #[serde(default)]
    pub(crate) runtime: VrlRuntime,
}

impl_generate_config_from_default!(VrlConfig);

impl ConditionalConfig for VrlConfig {
    fn build(
        &self,
        enrichment_tables: &vector_lib::enrichment::TableRegistry,
    ) -> crate::Result<Condition> {
        // TODO(jean): re-add this to VRL
        // let constraint = TypeConstraint {
        //     allow_any: false,
        //     type_def: TypeDef {
        //         fallible: true,
        //         kind: value::Kind::Boolean,
        //         ..Default::default()
        //     },
        // };

        let functions = vrl::stdlib::all()
            .into_iter()
            .chain(vector_lib::enrichment::vrl_functions())
            .chain(vector_vrl_functions::all())
            .collect::<Vec<_>>();

        let state = TypeState::default();

        let mut config = CompileConfig::default();
        config.set_custom(enrichment_tables.clone());
        config.set_read_only();

        let CompilationResult {
            program,
            warnings,
            config: _,
        } = compile_vrl(&self.source, &functions, &state, config).map_err(|diagnostics| {
            Formatter::new(&self.source, diagnostics)
                .colored()
                .to_string()
        })?;

        if !program.final_type_info().result.is_boolean() {
            return Err("VRL conditions must return a boolean.".into());
        }

        if !warnings.is_empty() {
            let warnings = Formatter::new(&self.source, warnings).colored().to_string();
            warn!(message = "VRL compilation warning.", %warnings);
        }

        match self.runtime {
            VrlRuntime::Ast => Ok(Condition::Vrl(Vrl {
                program,
                source: self.source.clone(),
            })),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Vrl {
    pub(super) program: Program,
    pub(super) source: String,
}

impl Vrl {
    fn run(&self, event: Event) -> (Event, RuntimeResult) {
        let log_namespace = event
            .maybe_as_log()
            .map(|log| log.namespace())
            .unwrap_or(LogNamespace::Legacy);
        let mut target = VrlTarget::new(event, self.program.info(), false);
        // TODO: use timezone from remap config
        let timezone = TimeZone::default();

        let result = Runtime::default().resolve(&mut target, &self.program, &timezone);
        let original_event = match target.into_events(log_namespace) {
            TargetEvents::One(event) => event,
            _ => panic!("Event was modified in a condition. This is an internal compiler error."),
        };
        (original_event, result)
    }
}

impl Conditional for Vrl {
    fn check(&self, event: Event) -> (bool, Event) {
        let (event, result) = self.run(event);

        let result = result
            .map(|value| match value {
                Value::Boolean(boolean) => boolean,
                _ => panic!("VRL condition did not return a boolean type"),
            })
            .unwrap_or_else(|err| {
                emit!(VrlConditionExecutionError {
                    error: err.to_string().as_ref()
                });
                false
            });
        (result, event)
    }

    fn check_with_context(&self, event: Event) -> (Result<(), String>, Event) {
        let (event, result) = self.run(event);

        let value_result = result.map_err(|err| match err {
            Terminate::Abort(err) => {
                let err = Formatter::new(
                    &self.source,
                    vrl::diagnostic::Diagnostic::from(
                        Box::new(err) as Box<dyn vrl::diagnostic::DiagnosticMessage>
                    ),
                )
                .colored()
                .to_string();
                format!("source execution aborted: {}", err)
            }
            Terminate::Error(err) => {
                let err = Formatter::new(
                    &self.source,
                    vrl::diagnostic::Diagnostic::from(
                        Box::new(err) as Box<dyn vrl::diagnostic::DiagnosticMessage>
                    ),
                )
                .colored()
                .to_string();
                format!("source execution failed: {}", err)
            }
        });

        let value = match value_result {
            Ok(value) => value,
            Err(err) => {
                return (Err(err), event);
            }
        };

        let result = match value {
            Value::Boolean(v) if v => Ok(()),
            Value::Boolean(v) if !v => Err("source execution resolved to false".into()),
            _ => Err("source execution resolved to a non-boolean value".into()),
        };
        (result, event)
    }
}

#[cfg(test)]
mod test {
    use vector_lib::metric_tags;

    use super::*;
    use crate::{
        event::{Metric, MetricKind, MetricValue},
        log_event,
    };

    #[test]
    fn generate_config() {
        crate::test_util::test_generate_config::<VrlConfig>();
    }

    #[test]
    fn check_vrl() {
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
            // TODO: enable once we don't emit large diagnostics with colors when no tty is present.
            // (
            //     log_event![],
            //     "null",
            //     Err("\n\u{1b}[0m\u{1b}[1m\u{1b}[38;5;9merror\u{1b}[0m\u{1b}[1m: unexpected return value\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m┌─\u{1b}[0m :1:1\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n\u{1b}[0m\u{1b}[34m1\u{1b}[0m \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31mnull\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m^^^^\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31mgot: null\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[34mexpected: boolean\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m=\u{1b}[0m see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
            //     Ok(()),
            // ),
            // (
            //     log_event!["foo" => "string"],
            //     ".foo",
            //     Err("\n\u{1b}[0m\u{1b}[1m\u{1b}[38;5;9merror\u{1b}[0m\u{1b}[1m: unexpected return value\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m┌─\u{1b}[0m :1:1\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n\u{1b}[0m\u{1b}[34m1\u{1b}[0m \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m.foo\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m^^^^\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31mgot: any\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[34mexpected: boolean\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m=\u{1b}[0m see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
            //     Ok(()),
            // ),
            // (
            //     log_event![],
            //     ".",
            //     Err("n\u{1b}[0m\u{1b}[1m\u{1b}[38;5;9merror\u{1b}[0m\u{1b}[1m: unexpected return value\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m┌─\u{1b}[0m :1:1\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n\u{1b}[0m\u{1b}[34m1\u{1b}[0m \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m.\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m^\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[31mgot: any\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m \u{1b}[0m\u{1b}[34mexpected: boolean\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m│\u{1b}[0m\n  \u{1b}[0m\u{1b}[34m=\u{1b}[0m see language documentation at: https://vector.dev/docs/reference/vrl/\n\n"),
            //     Ok(()),
            // ),
            (
                Event::Metric(
                    Metric::new(
                        "zork",
                        MetricKind::Incremental,
                        MetricValue::Counter { value: 1.0 },
                    )
                    .with_namespace(Some("zerk"))
                    .with_tags(Some(metric_tags!("host" => "zoobub"))),
                ),
                r#".name == "zork" && .tags.host == "zoobub" && .kind == "incremental""#,
                Ok(()),
                Ok(()),
            ),
            (
                log_event![],
                r#""i_return_a_string""#,
                Err("VRL conditions must return a boolean.".into()),
                Ok(()),
            ),
        ];

        for (event, source, build, check) in checks {
            let source = source.to_owned();
            let config = VrlConfig {
                source,
                runtime: Default::default(),
            };

            assert_eq!(
                config
                    .build(&Default::default())
                    .map(|_| ())
                    .map_err(|e| e.to_string()),
                build
            );

            if let Ok(cond) = config.build(&Default::default()) {
                assert_eq!(
                    cond.check_with_context(event.clone()).0,
                    check.map_err(|e| e.to_string())
                );
            }
        }
    }
}
