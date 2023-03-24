use ::value::Value;
use rand::{thread_rng, Rng};
use std::ops::Range;
use vrl::prelude::*;

const INVALID_RANGE_ERR: &str = "max must be greater than min";

fn random_float(min: Value, max: Value) -> Resolved {
    let min = min.try_float()?;
    let max = max.try_float()?;

    if max <= min {
        return Err("max must be greater than min".into());
    }

    let f: f64 = thread_rng().gen_range(min..max);

    Ok(Value::Float(NotNan::new(f).expect("always a number")))
}

fn get_range(min: Value, max: Value) -> std::result::Result<Range<f64>, &'static str> {
    let min = min.try_float().expect("min must be a float");
    let max = max.try_float().expect("max must be a float");

    if max <= min {
        return Err(INVALID_RANGE_ERR);
    }

    Ok(min..max)
}

#[derive(Clone, Copy, Debug)]
pub struct RandomFloat;

impl Function for RandomFloat {
    fn identifier(&self) -> &'static str {
        "random_float"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "min",
                kind: kind::FLOAT,
                required: true,
            },
            Parameter {
                keyword: "max",
                kind: kind::FLOAT,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "generate a random float from 0.0 to 10.0",
            source: r#"
				f = random_float(0.0, 10.0)
				f >= 0 && f < 10
                "#,
            result: Ok("true"),
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

        if let (Some(min), Some(max)) = (min.as_value(), max.as_value()) {
            // check if range is valid
            let _ = get_range(min, max.clone()).map_err(|err| {
                vrl::function::Error::InvalidArgument {
                    keyword: "max",
                    value: max,
                    error: err,
                }
            })?;
        }

        Ok(RandomFloatFn { min, max }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct RandomFloatFn {
    min: Box<dyn Expression>,
    max: Box<dyn Expression>,
}

impl FunctionExpression for RandomFloatFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let min = self.min.resolve(ctx)?;
        let max = self.max.resolve(ctx)?;

        random_float(min, max)
    }

    fn type_def(&self, _state: &state::TypeState) -> TypeDef {
        match (self.min.as_value(), self.max.as_value()) {
            (Some(min), Some(max)) => {
                if get_range(min, max).is_ok() {
                    TypeDef::float().infallible()
                } else {
                    TypeDef::float().fallible()
                }
            }
            _ => TypeDef::float().fallible(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // positive tests are handled by examples

    test_function![
        random_float => RandomFloat;

        bad_range {
            args: func_args![min: value!(1.0), max: value!(1.0)],
            want: Err("invalid argument"),
            tdef: TypeDef::float().fallible(),
        }
    ];
}
