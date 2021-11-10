use tracing::{debug, error, info, trace, warn};
use vrl::prelude::*;

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
        _state: &state::Compiler,
        info: &FunctionCompileContext,
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
            span: info.span,
            value,
            level,
            rate_limit_secs,
        }))
    }
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
            Some(expr) => expr.resolve(ctx)?.try_integer()?,
            None => 1,
        };

        match self.level.as_ref() {
            b"trace" => {
                trace!(message = %value, internal_log_rate_secs = rate_limit_secs, vrl_position = self.span.start())
            }
            b"debug" => {
                debug!(message = %value, internal_log_rate_secs = rate_limit_secs, vrl_position = self.span.start())
            }
            b"warn" => {
                warn!(message = %value, internal_log_rate_secs = rate_limit_secs, vrl_position = self.span.start())
            }
            b"error" => {
                error!(message = %value, internal_log_rate_secs = rate_limit_secs, vrl_position = self.span.start())
            }
            _ => {
                info!(message = %value, internal_log_rate_secs = rate_limit_secs, vrl_position = self.span.start())
            }
        }

        Ok(SharedValue::from(Value::Null))
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        log => Log;

        doesnotcrash {
            args: func_args! [ value: value!(42),
                               level: value!("warn"),
                               rate_limit_secs: value!(5) ],
            want: Ok(Value::Null),
            tdef: TypeDef::new().infallible().null(),
        }
    ];
}
