use ::value::Value;
use rand::{thread_rng, Rng};
use vrl::prelude::*;

fn random_float(min: Value, max: Value) -> Resolved {
    let min = min.try_float()?;
    let max = max.try_float()?;

    if max <= min {
        return Err("max must be greater than min".into());
    }

    let f: f64 = thread_rng().gen_range(min..max);

    Ok(Value::Float(NotNan::new(f).expect("always a number")))
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
            title: "generate random float from 0 to 10",
            source: r#"random_float(0, 10)"#,
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
        TypeDef::float().infallible()
    }
}

#[cfg(test)]
mod tests {
    // cannot test since non-deterministic
}
