use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn boolean(value: Value) -> Resolved {
    match value {
        v @ Value::Boolean(_) => Ok(v),
        v => Err(format!("expected boolean, got {}", v.kind()).into()),
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
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(BooleanFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct BooleanFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for BooleanFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        boolean(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        let non_boolean = !self.value.type_def(state).is_boolean();

        TypeDef::boolean().with_fallibility(non_boolean)
    }
}
