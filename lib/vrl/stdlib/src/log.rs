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

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
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
            .unwrap_bytes();

        Ok(Box::new(LogFn { value, level }))
    }
}

#[derive(Debug, Clone)]
struct LogFn {
    value: Box<dyn Expression>,
    level: Bytes,
}

impl Expression for LogFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        match self.level.as_ref() {
            b"trace" => trace!("{}", value),
            b"debug" => debug!("{}", value),
            b"warn" => warn!("{}", value),
            b"error" => error!("{}", value),
            _ => info!("{}", value),
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
