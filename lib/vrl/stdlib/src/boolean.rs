use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Boolean(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::boolean(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Boolean;

impl Function for Boolean {
    fn identifier(&self) -> &'static str {
        "bool"
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
                source: r#"bool(false)"#,
                result: Ok("false"),
            },
            Example {
                title: "invalid",
                source: "bool!(42)",
                result: Err(
                    r#"function call error for "bool" at (0:9): expected boolean, got integer"#,
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

        Ok(Box::new(BooleanFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct BooleanFn {
    value: Box<dyn Expression>,
}

impl Expression for BooleanFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx mut Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_boolean = !self.value.type_def(state).is_boolean();

        TypeDef::boolean().with_fallibility(non_boolean)
    }
}
