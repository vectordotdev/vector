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
            Value::Object(_) => Ok(value!("object")),
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use regex::Regex;

    test_function![
        type_of => TypeOf;

        bytes {
            args: func_args![value: value!("foobar")],
            want: Ok(value!("bytes")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        integer {
            args: func_args![value: value!(1789)],
            want: Ok(value!("integer")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        float {
            args: func_args![value: value!(0.577215664)],
            want: Ok(value!("float")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        boolean {
            args: func_args![value: value!(true)],
            want: Ok(value!("boolean")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        object {
            args: func_args![value: value!({"foo": "bar"})],
            want: Ok(value!("object")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        array {
            args: func_args![value: value!([1, 5, 1, 5])],
            want: Ok(value!("array")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        timestamp {
            args: func_args![value: value!(DateTime::parse_from_rfc2822("Wed, 17 Mar 2021 12:00:00 +0000")
                .unwrap()
                .with_timezone(&Utc))],
            want: Ok(value!("timestamp")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        regex {
            args: func_args![value: value!(Regex::new(r"\d+").unwrap())],
            want: Ok(value!("regex")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        null {
            args: func_args![value: value!(null)],
            want: Ok(Value::Bytes(bytes::Bytes::from("null"))),
            tdef: TypeDef::new().infallible().bytes(),
        }

        empty_string {
            args: func_args![value: value!("")],
            want: Ok(value!("bytes")),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
