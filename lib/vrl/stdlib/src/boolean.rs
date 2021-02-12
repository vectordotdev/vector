use vrl::prelude::*;

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
                    r#"function call error for "bool" at (0:9): expected "boolean", got "integer""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(BooleanFn { value }))
    }
}

#[derive(Debug, Clone)]
struct BooleanFn {
    value: Box<dyn Expression>,
}

impl Expression for BooleanFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.value.resolve(ctx)? {
            v @ Value::Boolean(_) => Ok(v),
            v => Err(format!(r#"expected "boolean", got {}"#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Boolean)
            .boolean()
    }
}
