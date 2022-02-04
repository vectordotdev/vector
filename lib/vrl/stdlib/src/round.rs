use vrl::prelude::*;

use crate::util::round_to_precision;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let precision = arguments.optional("precision").unwrap_or(expr!(0));

        Ok(Box::new(RoundFn { value, precision }))
    }
}

#[derive(Debug, Clone)]
struct RoundFn {
    value: Box<dyn Expression>,
    precision: Box<dyn Expression>,
}

impl Expression for RoundFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = self.precision.resolve(ctx)?.try_integer()?;

        match self.value.resolve(ctx)? {
            Value::Float(f) => Ok(round_to_precision(f.into_inner(), precision, f64::round).into()),
            value @ Value::Integer(_) => Ok(value),
            value => Err(value::Error::Expected {
                got: value.kind(),
                expected: Kind::Float | Kind::Integer,
            }
            .into()),
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
        round => Round;

        down {
             args: func_args![value: 1234.2],
             want: Ok(1234.0),
             tdef: TypeDef::new().infallible().integer(),
         }

        up {
             args: func_args![value: 1234.8],
             want: Ok(1235.0),
             tdef: TypeDef::new().infallible().integer(),
         }

        integer {
             args: func_args![value: 1234],
             want: Ok(1234),
             tdef: TypeDef::new().infallible().integer(),
         }

        precision {
             args: func_args![value: 1234.39429,
                              precision: 1
             ],
             want: Ok(1234.4),
             tdef: TypeDef::new().infallible().integer(),
         }

        bigger_precision  {
            args: func_args![value: 1234.56789,
                             precision: 4
            ],
            want: Ok(1234.5679),
            tdef: TypeDef::new().infallible().integer(),
        }

        huge {
             args: func_args![value: 9876543210123456789098765432101234567890987654321.987654321,
                              precision: 5
             ],
             want: Ok(9876543210123456789098765432101234567890987654321.98765),
             tdef: TypeDef::new().infallible().integer(),
         }
    ];
}
