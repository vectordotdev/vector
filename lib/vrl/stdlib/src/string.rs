use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Bytes(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::bytes(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct String;

impl Function for String {
    fn identifier(&self) -> &'static str {
        "string"
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
                source: r#"string("foobar")"#,
                result: Ok("foobar"),
            },
            Example {
                title: "invalid",
                source: "string!(true)",
                result: Err(
                    r#"function call error for "string" at (0:13): expected string, got boolean"#,
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

        Ok(Box::new(StringFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, arguments: &mut VmArgumentList) -> Result<Value> {
        let value = arguments.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct StringFn {
    value: Box<dyn Expression>,
}

impl Expression for StringFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_bytes = !self.value.type_def(state).is_bytes();

        TypeDef::bytes().with_fallibility(non_bytes)
    }
}
