use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Timestamp(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::timestamp(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Timestamp;

impl Function for Timestamp {
    fn identifier(&self) -> &'static str {
        "timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "valid",
                source: r#"to_string(timestamp(t'2021-02-11 21:42:01Z'))"#,
                result: Ok(r#""2021-02-11T21:42:01Z""#),
            },
            Example {
                title: "invalid",
                source: "timestamp!(true)",
                result: Err(
                    r#"function call error for "timestamp" at (0:16): expected timestamp, got boolean"#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(TimestampFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct TimestampFn {
    value: Box<dyn Expression>,
}

impl Expression for TimestampFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_timestamp = !self.value.type_def(state).is_timestamp();

        TypeDef::timestamp().with_fallibility(non_timestamp)
    }
}
