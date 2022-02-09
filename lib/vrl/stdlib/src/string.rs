use vrl::prelude::*;

fn string(value: Value) -> std::result::Result<Value, ExpressionError> {
    match value {
        v @ Value::Bytes(_) => Ok(v),
        v => Err(format!(r#"expected "string", got {}"#, v.kind()).into()),
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
                    r#"function call error for "string" at (0:13): expected "string", got "boolean""#,
                ),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(StringFn { value }))
    }

    fn call_by_vm(&self, _ctx: &mut Context, arguments: &mut VmArgumentList) -> Resolved {
        let value = arguments.required("value");
        string(value)
    }
}

#[derive(Debug, Clone)]
struct StringFn {
    value: Box<dyn Expression>,
}

impl Expression for StringFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        string(self.value.resolve(ctx)?)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .bytes()
    }
}
