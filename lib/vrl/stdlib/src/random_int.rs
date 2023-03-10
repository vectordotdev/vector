use ::value::Value;
use rand::{thread_rng, Rng};
use vrl::prelude::*;

fn random_int(min: Value, max: Value) -> Resolved {
    let min = min.try_integer()?;
    let max = max.try_integer()?;

    if max <= min {
        return Err("max must be greater than min".into());
    }

    let i: i64 = thread_rng().gen_range(min..max);

    Ok(Value::Integer(i))
}

#[derive(Clone, Copy, Debug)]
pub struct RandomInt;

impl Function for RandomInt {
    fn identifier(&self) -> &'static str {
        "random_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "min",
                kind: kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "max",
                kind: kind::INTEGER,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "generate random integer from 0 to 10",
            source: r#"random_int(0, 10)"#,
            result: Ok("2"),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let min = arguments.required("min");
        let max = arguments.required("max");

        Ok(RandomIntFn { min, max }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct RandomIntFn {
    min: Box<dyn Expression>,
    max: Box<dyn Expression>,
}

impl FunctionExpression for RandomIntFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let min = self.min.resolve(ctx)?;
        let max = self.max.resolve(ctx)?;

        random_int(min, max)
    }

    fn type_def(&self, _state: &state::TypeState) -> TypeDef {
        TypeDef::integer().fallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        random_int => RandomInt;

        random {
            args: func_args![min: value!(1), max: value!(2)],
            want: Ok(value!(1)),
            tdef: TypeDef::integer().fallible(),
        }
    ];
}
