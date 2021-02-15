use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Length;

impl Function for Length {
    fn identifier(&self) -> &'static str {
        "length"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ARRAY | kind::OBJECT | kind::BYTES,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "array",
                source: r#"length([0, 1])"#,
                result: Ok("2"),
            },
            Example {
                title: "object",
                source: r#"length({ "foo": "bar"})"#,
                result: Ok("1"),
            },
            Example {
                title: "string",
                source: r#"length("foobar")"#,
                result: Ok("6"),
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(LengthFn { value }))
    }
}

#[derive(Debug, Clone)]
struct LengthFn {
    value: Box<dyn Expression>,
}

impl Expression for LengthFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = self.value.resolve(ctx)?;

        match value {
            Array(v) => Ok(v.len().into()),
            Object(v) => Ok(v.len().into()),
            Bytes(v) => Ok(v.len().into()),
            _ => unreachable!("argument-type invariant"),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().integer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        length => Length;

        non_empty_object_value {
            args: func_args![value: value!({"foo": "bar", "baz": true, "baq": [1, 2, 3]})],
            want: Ok(value!(3)),
            tdef: TypeDef::new().infallible().integer(),
        }

        empty_object_value {
            args: func_args![value: value!({})],
            want: Ok(value!(0)),
            tdef: TypeDef::new().infallible().integer(),
        }

        nested_object_value {
            args: func_args![value: value!({"nested": {"foo": "bar"}})],
            want: Ok(value!(1)),
            tdef: TypeDef::new().infallible().integer(),
        }

        non_empty_array_value {
            args: func_args![value: value!([1, 2, 3, 4, true, "hello"])],
            want: Ok(value!(6)),
            tdef: TypeDef::new().infallible().integer(),
        }

        empty_array_value {
            args: func_args![value: value!([])],
            want: Ok(value!(0)),
            tdef: TypeDef::new().infallible().integer(),
        }

        string_value {
            args: func_args![value: value!("hello world")],
            want: Ok(value!(11)),
            tdef: TypeDef::new().infallible().integer(),
        }
    ];
}
