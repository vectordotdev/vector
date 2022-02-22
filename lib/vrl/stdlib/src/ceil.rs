use vrl::prelude::*;

use crate::util::round_to_precision;

#[derive(Clone, Copy, Debug)]
pub struct Ceil;

impl Function for Ceil {
    fn identifier(&self) -> &'static str {
        "ceil"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::FLOAT | kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "precision",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let precision = arguments.optional("precision");

        Ok(Box::new(CeilFn { value, precision }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "ceil",
            source: r#"ceil(5.2)"#,
            result: Ok("6.0"),
        }]
    }
}

#[derive(Clone, Debug)]
struct CeilFn {
    value: Box<dyn Expression>,
    precision: Option<Box<dyn Expression>>,
}

impl Expression for CeilFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = match &self.precision {
            Some(expr) => expr.resolve(ctx)?.try_integer()?,
            None => 0,
        };

        match self.value.resolve(ctx)? {
            Value::Float(f) => Ok(Value::from_f64_or_zero(round_to_precision(
                *f,
                precision,
                f64::ceil,
            ))),
            value @ Value::Integer(_) => Ok(value),
            value => Err(value::Error::Expected {
                got: value.kind(),
                expected: Kind::float() | Kind::integer(),
            }
            .into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
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
        ceil => Ceil;

        lower {
            args: func_args![value: value!(1234.2)],
            want: Ok(value!(1235.0)),
            tdef: TypeDef::float(),
        }

        higher {
            args: func_args![value: value!(1234.8)],
            want: Ok(value!(1235.0)),
            tdef: TypeDef::float(),
        }

        integer {
            args: func_args![value: value!(1234)],
            want: Ok(value!(1234)),
            tdef: TypeDef::integer(),
        }

        precision {
            args: func_args![value: value!(1234.39429),
                             precision: value!(1)
            ],
            want: Ok(value!(1234.4)),
            tdef: TypeDef::float(),
        }

        bigger_precision {
            args: func_args![value: value!(1234.56725),
                             precision: value!(4)
            ],
            want: Ok(value!(1234.5673)),
            tdef: TypeDef::float(),
        }

        huge_number {
             args: func_args![value: value!(9876543210123456789098765432101234567890987654321.987654321),
                             precision: value!(5)
            ],
            want: Ok(value!(9876543210123456789098765432101234567890987654321.98766)),
            tdef: TypeDef::float(),
        }
    ];
}
