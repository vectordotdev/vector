use remap::prelude::*;
use tracing::{debug, error, info, trace, warn};

#[derive(Clone, Copy, Debug)]
pub struct Log;

const LEVELS: &[&str] = &["trace", "debug", "info", "warn", "error"];

impl Function for Log {
    fn identifier(&self) -> &'static str {
        "log"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |_| true,
                required: true,
            },
            Parameter {
                keyword: "level",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "rate_limit_secs",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let level = arguments
            .optional_enum("level", &LEVELS)?
            .unwrap_or_else(|| "info".to_string());
        let rate_limit_secs = arguments.optional("rate_limit_secs").map(Expr::boxed);

        Ok(Box::new(LogFn {
            value,
            level,
            rate_limit_secs,
        }))
    }
}

#[derive(Debug, Clone)]
struct LogFn {
    value: Box<dyn Expression>,
    level: String,
    rate_limit_secs: Option<Box<dyn Expression>>,
}

impl LogFn {
    #[cfg(test)]
    fn new(
        value: Box<dyn Expression>,
        level: String,
        rate_limit_secs: Option<Box<dyn Expression>>,
    ) -> Self {
        Self {
            value,
            level,
            rate_limit_secs,
        }
    }
}

impl Expression for LogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;
        let rate_limit_secs = match &self.rate_limit_secs {
            Some(expr) => expr.execute(state, object)?.try_integer()?,
            None => 1,
        };

        match self.level.as_ref() {
            "trace" => trace!(internal_log_rate_secs = rate_limit_secs, "{}", value),
            "debug" => debug!(internal_log_rate_secs = rate_limit_secs, "{}", value),
            "warn" => warn!(internal_log_rate_secs = rate_limit_secs, "{}", value),
            "error" => error!(internal_log_rate_secs = rate_limit_secs, "{}", value),
            _ => info!(internal_log_rate_secs = rate_limit_secs, "{}", value),
        }

        Ok(Value::Null)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .merge_optional(
                self.rate_limit_secs
                    .as_ref()
                    .map(|r| r.type_def(state).fallible_unless(value::Kind::Integer)),
            )
            .with_constraint(value::Kind::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    remap::test_type_def! [

        rate_limit_integer {
            expr: |_| LogFn {
                value: Literal::from("foo").boxed(),
                level: "error".to_string(),
                rate_limit_secs: Some(Literal::from(30).boxed()),
            },
            def: TypeDef { kind: value::Kind::Null, fallible: false, ..Default::default() },
        }

        rate_limit_non_integer {
            expr: |_| LogFn {
                value: Literal::from("foo").boxed(),
                level: "error".to_string(),
                rate_limit_secs: Some(Literal::from("bar").boxed()),
            },
            def: TypeDef { kind: value::Kind::Null, fallible: true, ..Default::default() },
        }
    ];

    #[test]
    fn log() {
        // This is largely just a smoke test to ensure it doesn't crash as there isn't really much to test.
        let mut state = state::Program::default();
        let func = LogFn::new(
            Box::new(Array::from(vec![Value::from(42)])),
            "warn".to_string(),
            None,
        );
        let mut object = Value::Map(btreemap! {});
        let got = func.execute(&mut state, &mut object);

        assert_eq!(Ok(Value::Null), got);
    }
}
