use ::value::Value;
use vrl::prelude::*;

use crate::util::round_to_precision;

fn round(precision: Value, value: Value) -> Resolved {
    let precision = precision.try_integer()?;
    match value {
        Value::Float(f) => Ok(Value::from_f64_or_zero(round_to_precision(
            f.into_inner(),
            precision,
            f64::round,
        ))),
        value @ Value::Integer(_) => Ok(value),
        value => Err(value::Error::Expected {
            got: value.kind(),
            expected: Kind::float() | Kind::integer(),
        }
        .into()),
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Round;

impl Function for Round {
    fn identifier(&self) -> &'static str {
        "round"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::INTEGER | kind::FLOAT,
                required: true,
            },
            Parameter {
                keyword: "precision",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "round up",
                source: r#"round(5.5)"#,
                result: Ok("6.0"),
            },
            Example {
                title: "round down",
                source: r#"round(5.45)"#,
                result: Ok("5.0"),
            },
            Example {
                title: "precision",
                source: r#"round(5.45, 1)"#,
                result: Ok("5.5"),
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
        let precision = arguments.optional("precision").unwrap_or(expr!(0));

        Ok(RoundFn { value, precision }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct RoundFn {
    value: Box<dyn Expression>,
    precision: Box<dyn Expression>,
}

impl FunctionExpression for RoundFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = self.precision.resolve(ctx)?;
        let value = self.value.resolve(ctx)?;

        round(precision, value)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::integer().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        round => Round;

        down {
             args: func_args![value: 1234.2],
             want: Ok(1234.0),
             tdef: TypeDef::integer().infallible(),
         }

        up {
             args: func_args![value: 1234.8],
             want: Ok(1235.0),
             tdef: TypeDef::integer().infallible(),
         }

        integer {
             args: func_args![value: 1234],
             want: Ok(1234),
             tdef: TypeDef::integer().infallible(),
         }

        precision {
             args: func_args![value: 1234.39429,
                              precision: 1
             ],
             want: Ok(1234.4),
             tdef: TypeDef::integer().infallible(),
         }

        bigger_precision  {
            args: func_args![value: 1234.56789,
                             precision: 4
            ],
            want: Ok(1234.5679),
            tdef: TypeDef::integer().infallible(),
        }

        huge {
             args: func_args![value: 9_876_543_210_123_456_789_098_765_432_101_234_567_890_987_654_321.987_654_321,
                              precision: 5
             ],
             want: Ok(9_876_543_210_123_456_789_098_765_432_101_234_567_890_987_654_321.987_65),
             tdef: TypeDef::integer().infallible(),
         }
    ];
}
