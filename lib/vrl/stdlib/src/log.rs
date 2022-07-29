use std::any::Any;

use ::value::Value;
use primitive_calling_convention::primitive_calling_convention;
use tracing::{debug, error, info, trace, warn};
use vrl::prelude::*;

fn log(value: Value, rate_limit_secs: Value, info: &LogInfo) -> Resolved {
    let rate_limit_secs = rate_limit_secs.try_integer()?;
    let res = value.to_string_lossy();
    let LogInfo { level, span } = info;
    match level.as_ref() {
        b"trace" => {
            trace!(message = %res, internal_log_rate_secs = rate_limit_secs, vrl_position = span.start())
        }
        b"debug" => {
            debug!(message = %res, internal_log_rate_secs = rate_limit_secs, vrl_position = span.start())
        }
        b"warn" => {
            warn!(message = %res, internal_log_rate_secs = rate_limit_secs, vrl_position = span.start())
        }
        b"error" => {
            error!(message = %res, internal_log_rate_secs = rate_limit_secs, vrl_position = span.start())
        }
        _ => {
            info!(message = %res, internal_log_rate_secs = rate_limit_secs, vrl_position = span.start())
        }
    }
    Ok(Value::Null)
}

fn levels() -> Vec<Bytes> {
    vec![
        Bytes::from("trace"),
        Bytes::from("debug"),
        Bytes::from("info"),
        Bytes::from("warn"),
        Bytes::from("error"),
    ]
}

#[derive(Clone, Copy, Debug)]
pub struct Log;

impl Function for Log {
    fn identifier(&self) -> &'static str {
        "log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "level",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "rate_limit_secs",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "default log level (info)",
                source: r#"log("foo")"#,
                result: Ok("null"),
            },
            Example {
                title: "custom level",
                source: r#"log("foo", "error")"#,
                result: Ok("null"),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let levels = vec![
            "trace".into(),
            "debug".into(),
            "info".into(),
            "warn".into(),
            "error".into(),
        ];

        let value = arguments.required("value");
        let level = arguments
            .optional_enum("level", &levels)?
            .unwrap_or_else(|| "info".into())
            .try_bytes()
            .expect("log level not bytes");
        let rate_limit_secs = arguments.optional("rate_limit_secs");

        Ok(Box::new(LogFn {
            info: LogInfo {
                level,
                span: ctx.span(),
            },
            value,
            rate_limit_secs,
        }))
    }

    fn compile_argument(
        &self,
        _state: (&state::LocalEnv, &state::ExternalEnv),
        _args: &[(&'static str, Option<ResolvedArgument>)],
        ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        if name == "level" {
            let level = match expr {
                Some(expr) => match expr.as_value() {
                    Some(value) => Some(
                        levels()
                            .into_iter()
                            .find(|level| Some(level) == value.as_bytes())
                            .ok_or_else(|| vrl::function::Error::InvalidEnumVariant {
                                keyword: "level",
                                value,
                                variants: levels().into_iter().map(Value::from).collect::<Vec<_>>(),
                            })?,
                    ),
                    None => None,
                },
                None => None,
            }
            .unwrap_or_else(|| Bytes::from("info"));

            let level = LogInfo {
                level,
                span: ctx.span(),
            };
            Ok(Some(Box::new(level) as Box<dyn std::any::Any + Send + Sync>))
        } else {
            Ok(None)
        }
    }

    fn symbol(&self) -> Option<Symbol> {
        Some(Symbol {
            name: "vrl_fn_log",
            address: vrl_fn_log as _,
            uses_context: false,
        })
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
struct LogInfo {
    level: Bytes,
    span: vrl::diagnostic::Span,
}

#[derive(Debug, Clone)]
struct LogFn {
    info: LogInfo,
    value: Box<dyn Expression>,
    rate_limit_secs: Option<Box<dyn Expression>>,
}

impl Expression for LogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let rate_limit_secs = match &self.rate_limit_secs {
            Some(expr) => expr.resolve(ctx)?,
            None => value!(1),
        };

        log(value, rate_limit_secs, &self.info)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
}

#[no_mangle]
#[primitive_calling_convention]
extern "C" fn vrl_fn_log(
    value: Value,
    info: &Box<dyn Any + Send + Sync>,
    rate_limit_secs: Option<Value>,
) -> Resolved {
    let info = info.downcast_ref::<LogInfo>().unwrap();
    let rate_limit_secs = rate_limit_secs.unwrap_or_else(|| 1.into());

    log(value, rate_limit_secs, info)
}

#[cfg(test)]
mod tests {
    use tracing_test::traced_test;

    use super::*;

    test_function![
        log => Log;

        doesnotcrash {
            args: func_args! [ value: value!(42),
                               level: value!("warn"),
                               rate_limit_secs: value!(5) ],
            want: Ok(Value::Null),
            tdef: TypeDef::null().infallible(),
        }
    ];

    #[traced_test]
    #[test]
    fn output_quotes() {
        // Check that a message is logged without additional quotes
        log(
            value!("simple test message"),
            value!(1),
            &LogInfo {
                level: Bytes::from("warn"),
                span: Default::default(),
            },
        )
        .unwrap();

        assert!(!logs_contain("\"simple test message\""));
        assert!(logs_contain("simple test message"));
    }
}
