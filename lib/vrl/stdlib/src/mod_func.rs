use ::value::Value;
use vrl::prelude::*;

fn r#mod(value: Value, modulus: Value) -> Resolved {
    let result = value.try_rem(modulus)?;
    Ok(result)
}

#[derive(Clone, Copy, Debug)]
pub struct Mod;

impl Function for Mod {
    fn identifier(&self) -> &'static str {
        "mod"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::INTEGER | kind::FLOAT,
                required: true,
            },
            Parameter {
                keyword: "modulus",
                kind: kind::INTEGER | kind::FLOAT,
                required: true,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "mod",
            source: r#"mod(5, 3)"#,
            result: Ok("2"),
        }]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let modulus = arguments.required("modulus");
        // TODO: return a compile-time error if modulus is 0

        Ok(ModFn { value, modulus }.as_expr())
    }
}

#[derive(Debug, Clone)]
struct ModFn {
    value: Box<dyn Expression>,
    modulus: Box<dyn Expression>,
}

impl FunctionExpression for ModFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let modulus = self.modulus.resolve(ctx)?;
        r#mod(value, modulus)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        // Division is infallible if the rhs is a literal normal float or a literal non-zero integer.
        match self.modulus.as_value() {
            Some(value) if value.is_float() || value.is_integer() => match value {
                Value::Float(v) if v.is_normal() => TypeDef::float().infallible(),
                Value::Float(_) => TypeDef::float().fallible(),
                Value::Integer(v) if v != 0 => TypeDef::integer().infallible(),
                Value::Integer(_) => TypeDef::integer().fallible(),
                _ => TypeDef::float().or_integer().fallible(),
            },
            _ => TypeDef::float().or_integer().fallible(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        r#mod => Mod;

        int_mod {
            args: func_args![value: 5, modulus: 2],
            want: Ok(value!(1)),
            tdef: TypeDef::integer().infallible(),
        }

        float_mod {
            args: func_args![value: 5.0, modulus: 2.0],
            want: Ok(value!(1.0)),
            tdef: TypeDef::float().infallible(),
        }

        fallible_mod {
            args: func_args![value: 5.0, modulus: {}],
            want: Err("can't calculate remainder of type float and null"),
            tdef: TypeDef::float().or_integer().fallible(),
        }
    ];
}
