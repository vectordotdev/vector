use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Float(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::float(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Float;

impl Function for Float {
    fn identifier(&self) -> &'static str {
        "float"
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
                source: r#"float(3.1415)"#,
                result: Ok("3.1415"),
            },
            Example {
                title: "invalid",
                source: "float!(true)",
                result: Err(
                    r#"function call error for "float" at (0:12): expected float, got boolean"#,
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

        Ok(Box::new(FloatFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct FloatFn {
    value: Box<dyn Expression>,
}

impl Expression for FloatFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_float = !self.value.type_def(state).is_float();

        TypeDef::float().with_fallibility(non_float)
    }
}
