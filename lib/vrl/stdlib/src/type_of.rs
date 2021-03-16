use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct TypeOf;

impl Function for TypeOf {
    fn identifier(&self) -> &'static str {
        "type_of"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::ANY,
            required: true,
        }]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "null",
            source: r#"type_of(null)"#,
            result: Ok("null"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        Ok(Box::new(TypeOfFn { value }))
    }
}

#[derive(Clone, Debug)]
struct TypeOfFn {
    value: Box<dyn Expression>,
}

impl Expression for TypeOfFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let v = self.value.resolve(ctx)?;
        match v {
            Value::Bytes(_) => Ok(value!("bytes")),
            Value::Integer(_) => Ok(value!("integer")),
            Value::Float(_) => Ok(value!("float")),
            Value::Boolean(_) => Ok(value!("boolean")),
            Value::Object(_) => Ok(value!("timestamp")),
            Value::Array(_) => Ok(value!("array")),
            Value::Timestamp(_) => Ok(value!("timestamp")),
            Value::Regex(_) => Ok(value!("regex")),
            Value::Null => Ok(Value::Bytes(bytes::Bytes::from("null"))),
        }
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

/*#[cfg(test)]
mod tests {
    use super::*;
    test_function![
        is_nullish => IsNullish;

        empty_string {
            args: func_args![value: value!("")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        single_space_string {
            args: func_args![value: value!(" ")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        multi_space_string {
            args: func_args![value: value!("     ")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        newline_string {
            args: func_args![value: value!("\n")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        carriage_return_string {
            args: func_args![value: value!("\r")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        dash_string {
            args: func_args![value: value!("-")],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        null {
            args: func_args![value: value!(null)],
            want: Ok(value!(true)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        non_empty_string {
            args: func_args![value: value!("hello world")],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        // Shows that a non-string/null literal returns false
        integer {
            args: func_args![value: value!(427)],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }

        // Shows that a non-literal type returns false
        array {
            args: func_args![value: value!([1, 2, 3])],
            want: Ok(value!(false)),
            tdef: TypeDef::new().infallible().boolean(),
        }
    ];
}*/
