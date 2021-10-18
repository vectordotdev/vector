use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct IsEmpty;

impl Function for IsEmpty {
    fn identifier(&self) -> &'static str {
        "is_empty"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ARRAY | kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "empty string",
                source: r#"is_empty("")"#,
                result: Ok("true"),
            },
            Example {
                title: "empty array",
                source: r#"is_empty([])"#,
                result: Ok("true"),
            },
            Example {
                title: "non-empty array",
                source: r#"is_empty([null])"#,
                result: Ok("false"),
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

        Ok(Box::new(IsEmptyFn { value }))
    }
}

#[derive(Debug, Clone)]
struct IsEmptyFn {
    value: Box<dyn Expression>,
}

impl Expression for IsEmptyFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let empty = match self.value.resolve(ctx)? {
            Value::Array(v) => v.is_empty(),
            Value::Bytes(v) => v.is_empty(),
            value => {
                return Err(value::Error::Expected {
                    got: value.kind(),
                    expected: Kind::Bytes | Kind::Array,
                }
                .into())
            }
        };

        Ok(empty.into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().boolean()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        is_empty => IsEmpty;

        empty_array {
            args: func_args![value: value!([])],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        non_empty_array {
            args: func_args![value: value!(["foo"])],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        empty_string {
            args: func_args![value: ""],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        non_empty_string {
            args: func_args![value: "foo"],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}
