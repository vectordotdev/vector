use vrl::prelude::*;

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
                    r#"function call error for "float" at (0:12): expected "float", got "boolean""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(FloatFn { value }))
    }
}

#[derive(Debug, Clone)]
struct FloatFn {
    value: Box<dyn Expression>,
}

impl Expression for FloatFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.value.resolve(ctx)? {
            v @ Value::Float(_) => Ok(v),
            v => Err(format!(r#"expected "float", got {}"#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Float)
            .float()
    }
}
