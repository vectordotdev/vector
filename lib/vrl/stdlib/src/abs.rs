use ::value::Value;
use rust_decimal::prelude::Signed;
use vrl::prelude::expression::FunctionExpression;
use vrl::prelude::*;

fn abs(value: Value) -> Resolved {
    match value {
        Value::Float(f) => Ok(Value::from_f64_or_zero(*f.abs())),
        Value::Integer(i) => Ok(Value::from(i.abs())),
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::float() | Kind::integer(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Abs;

impl Function for Abs {
    fn identifier(&self) -> &'static str {
        "abs"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            kind: kind::FLOAT | kind::INTEGER,
            required: true,
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");

        Ok(AbsFn { value }.as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "abs",
            source: r#"abs(-42)"#,
            result: Ok("42"),
        }]
    }
}

#[derive(Clone, Debug)]
struct AbsFn {
    value: Box<dyn Expression>,
}

impl FunctionExpression for AbsFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        abs(value)
    }

    fn type_def(&self, state: &state::TypeState) -> TypeDef {
        match Kind::from(self.value.type_def(state)) {
            v if v.is_float() || v.is_integer() => v.into(),
            _ => Kind::integer().or_float().into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        abs => Abs;

        integer_negative {
            args: func_args![value: value!(-42)],
            want: Ok(value!(42)),
            tdef: TypeDef::integer(),
        }

        integer_positive {
            args: func_args![value: value!(42)],
            want: Ok(value!(42)),
            tdef: TypeDef::integer(),
        }

        float_negative {
            args: func_args![value: value!(-42.2)],
            want: Ok(value!(42.2)),
            tdef: TypeDef::float(),
        }

        float_positive {
            args: func_args![value: value!(42.2)],
            want: Ok(value!(42.2)),
            tdef: TypeDef::float(),
        }
    ];
}
