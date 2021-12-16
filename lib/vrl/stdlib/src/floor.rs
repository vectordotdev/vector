use vrl::prelude::*;

use crate::util::round_to_precision;

#[derive(Clone, Copy, Debug)]
pub struct Floor;

impl Function for Floor {
    fn identifier(&self) -> &'static str {
        "floor"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "precision",
                kind: kind::ANY,
                required: false,
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
        let precision = arguments.optional("precision");

        Ok(Box::new(FloorFn { value, precision }))
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "floor",
            source: r#"floor(9.8)"#,
            result: Ok("9.0"),
        }]
    }
}

#[derive(Clone, Debug)]
struct FloorFn {
    value: Box<dyn Expression>,
    precision: Option<Box<dyn Expression>>,
}

impl Expression for FloorFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let precision = match &self.precision {
            Some(expr) => expr.resolve(ctx)?.try_integer()?,
            None => 0,
        };

        match self.value.resolve(ctx)? {
            Value::Float(f) => Ok(round_to_precision(*f, precision, f64::floor).into()),
            value @ Value::Integer(_) => Ok(value),
            value => Err(value::Error::Expected {
                got: value.kind(),
                expected: Kind::Float | Kind::Integer,
            }
            .into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        TypeDef::new().scalar(match self.value.type_def(state).kind() {
            v if v.is_float() || v.is_integer() => v,
            _ => Kind::Integer | Kind::Float,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        floor => Floor;

        lower {
            args: func_args![value: 1234.2],
            want: Ok(value!(1234.0)),
            tdef: TypeDef::new().float(),
        }

        higher {
            args: func_args![value: 1234.8],
            want: Ok(value!(1234.0)),
            tdef: TypeDef::new().float(),
        }

        exact {
            args: func_args![value: 1234],
            want: Ok(value!(1234)),
            tdef: TypeDef::new().integer(),
        }

        precision {
            args: func_args![value: 1234.39429,
                             precision: 1],
            want: Ok(value!(1234.3)),
            tdef: TypeDef::new().float(),
        }

        bigger_precision {
            args: func_args![value: 1234.56789,
                             precision: 4],
            want: Ok(value!(1234.5678)),
            tdef: TypeDef::new().float(),
        }

        huge_number {
            args: func_args![value: 9876543210123456789098765432101234567890987654321.987654321,
                             precision: 5],
            want: Ok(value!(9876543210123456789098765432101234567890987654321.98765)),
            tdef: TypeDef::new().float(),
        }
    ];
}
