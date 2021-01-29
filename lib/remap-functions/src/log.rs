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
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let level = arguments
            .optional_enum("level", &LEVELS)?
            .unwrap_or_else(|| "info".to_string());

        Ok(Box::new(LogFn { value, level }))
    }
}

#[derive(Debug, Clone)]
struct LogFn {
    value: Box<dyn Expression>,
    level: String,
}

impl LogFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, level: String) -> Self {
        Self { value, level }
    }
}

impl Expression for LogFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?;

        match self.level.as_ref() {
            "trace" => trace!("{}", value),
            "debug" => debug!("{}", value),
            "warn" => warn!("{}", value),
            "error" => error!("{}", value),
            _ => info!("{}", value),
        }

        Ok(Value::Null)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .with_constraint(value::Kind::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn log() {
        // This is largely just a smoke test to ensure it doesn't crash as there isn't really much to test.
        let mut state = state::Program::default();
        let func = LogFn::new(
            Box::new(Array::from(vec![Value::from(42)])),
            "warn".to_string(),
        );
        let mut object = Value::Map(btreemap! {});
        let got = func.execute(&mut state, &mut object);

        assert_eq!(Ok(Value::Null), got);
    }
}
