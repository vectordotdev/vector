use ::value::Value;
use tracing::{debug, error, info, trace, warn};
use vrl::prelude::*;

fn log(
    rate_limit_secs: Value,
    level: &Bytes,
    value: Value,
    span: vrl::diagnostic::Span,
) -> Resolved {
    let rate_limit_secs = rate_limit_secs.try_integer()?;
    let res = value.to_string_lossy();
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
            span: ctx.span(),
            value,
            level,
            rate_limit_secs,
        }))
    }

    fn compile_argument(
        &self,
        _args: &[(&'static str, Option<FunctionArgument>)],
        ctx: &mut FunctionCompileContext,
        name: &str,
        expr: Option<&expression::Expr>,
    ) -> CompiledArgument {
        if name == "level" {
            let level = match expr {
                Some(expr) => match expr.as_value() {
                    Some(value) => levels()
                        .into_iter()
                        .find(|level| Some(level) == value.as_bytes())
                        .ok_or_else(|| vrl::function::Error::InvalidEnumVariant {
                            keyword: "level",
                            value,
                            variants: levels().into_iter().map(Value::from).collect::<Vec<_>>(),
                        })?,
                    None => return Ok(None),
                },
                None => Bytes::from("info"),
            };

            let level = LogInfo {
                level,
                span: ctx.span(),
            };
            Ok(Some(Box::new(level) as Box<dyn std::any::Any + Send + Sync>))
        } else {
            Ok(None)
        }
    }
}

#[allow(unused)] // will be used by LLVM runtime
#[derive(Debug)]
struct LogInfo {
    level: Bytes,
    span: vrl::diagnostic::Span,
}

#[derive(Debug, Clone)]
struct LogFn {
    span: vrl::diagnostic::Span,
    value: Box<dyn Expression>,
    level: Bytes,
    rate_limit_secs: Option<Box<dyn Expression>>,
}

impl Expression for LogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let rate_limit_secs = match &self.rate_limit_secs {
            Some(expr) => expr.resolve(ctx)?,
            None => value!(1),
        };

        let span = self.span;

        log(rate_limit_secs, &self.level, value, span)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::null().infallible()
    }
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
            value!(1),
            &Bytes::from("warn"),
            value!("simple test message"),
            Default::default(),
        )
        .unwrap();

        assert!(!logs_contain("\"simple test message\""));
        assert!(logs_contain("simple test message"));
    }
}
