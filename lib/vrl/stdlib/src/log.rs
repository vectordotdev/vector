use tracing::{debug, error, info, info_span, trace, warn, warn_span};
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

    fn compile(&self, mut _arguments: ArgumentList) -> Compiled {
        unimplemented!()
    }

    fn compile_with_span(
        &self,
        span: vrl::diagnostic::Span,
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
            value,
            span,
            level,
            rate_limit_secs,
        }))
    }
}

#[derive(Debug, Clone)]
struct LogFn {
    value: Box<dyn Expression>,
    level: Bytes,
    span: vrl::diagnostic::Span,
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
            b"trace" => trace!("{}", value),
            b"debug" => debug!("{}", value),
            b"warn" => {
                let span = warn_span!("remap", vrl_position = &self.span.start());
                let _ = span.enter();
                warn!(message = %value, internal_log_rate_secs = rate_limit_secs)
            }
            b"error" => error!("{}", value),
            _ => {
                let span = info_span!("remap", vrl_position = &self.span.start());
                let _ = span.enter();
                info!(message = %value, internal_log_rate_secs = rate_limit_secs)
            }
        }

        Ok(Value::Null)
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().null()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;

//     #[test]
//     fn log() {
//         // This is largely just a smoke test to ensure it doesn't crash as there isn't really much to test.
//         let mut state = state::Program::default();
//         let func = LogFn::new(
//             Box::new(Array::from(vec![Value::from(42)])),
//             "warn".to_string(),
//         );
//         let mut object = Value::Map(map![]);
//         let got = func.resolve(&mut ctx);

//         assert_eq!(Ok(Value::Null), got);
//     }
// }
