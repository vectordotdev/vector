use ::value::Value;
use vrl::prelude::*;

#[inline]
fn check(value: &Value) -> Result<()> {
    match value {
        Value::Integer(_) => Ok(()),
        _ => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::integer(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Integer;

impl Function for Integer {
    fn identifier(&self) -> &'static str {
        "int"
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
                source: r#"int(42)"#,
                result: Ok("42"),
            },
            Example {
                title: "invalid",
                source: "int!(true)",
                result: Err(
                    r#"function call error for "int" at (0:10): expected integer, got boolean"#,
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

        Ok(Box::new(IntegerFn { value }))
    }

    fn call_by_vm(&self, _ctx: &Context, args: &mut VmArgumentList) -> Result<Value> {
        let value = args.required("value");
        check(&value)?;

        Ok(value)
    }
}

#[derive(Debug, Clone)]
struct IntegerFn {
    value: Box<dyn Expression>,
}

impl Expression for IntegerFn {
    fn resolve<'value, 'ctx: 'value, 'rt: 'ctx>(
        &'rt self,
        ctx: &'ctx Context,
    ) -> Resolved<'value> {
        let value = self.value.resolve(ctx)?;
        check(&value)?;

        Ok(value)
    }

    fn type_def(&self, state: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        let non_integer = !self.value.type_def(state).is_integer();

        TypeDef::integer().with_fallibility(non_integer)
    }
}
