use vrl::prelude::*;

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
                    r#"function call error for "int" at (0:10): expected "integer", got "boolean""#,
                ),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(IntegerFn { value }))
    }
}

#[derive(Debug, Clone)]
struct IntegerFn {
    value: Box<dyn Expression>,
}

impl Expression for IntegerFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        match self.value.resolve(ctx)? {
            v @ Value::Integer(_) => Ok(v),
            v => Err(format!(r#"expected "integer", got {}"#, v.kind()).into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Integer)
            .integer()
    }
}
