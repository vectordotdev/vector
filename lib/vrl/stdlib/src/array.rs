use ::value::Value;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn array(value: Value) -> Resolved {
    match value {
        v @ Value::Array(_) => Ok(v),
        v => Err(format!("expected array, got {}", v.kind()).into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Array;

impl Function for Array {
    fn identifier(&self) -> &'static str {
        "array"
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
                source: r#"array([1,2,3])"#,
                result: Ok("[1,2,3]"),
            },
            Example {
                title: "invalid",
                source: "array!(true)",
                result: Err(
                    r#"function call error for "array" at (0:12): expected array, got boolean"#,
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

        Ok(ArrayFn { value }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ArrayFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for ArrayFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        array(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::array(Collection::any()))
            .restrict_array()
    }
}
