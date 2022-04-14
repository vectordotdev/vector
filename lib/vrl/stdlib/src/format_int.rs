use std::collections::VecDeque;

use vrl::prelude::*;

fn format_int(value: Value, base: Option<Value>) -> Resolved {
    let value = value.try_integer()?;
    let base = match base {
        Some(base) => {
            let value = base.try_integer()?;
            if !(2..=36).contains(&value) {
                return Err(format!(
                    "invalid base {}: must be be between 2 and 36 (inclusive)",
                    value
                )
                .into());
            }

            value as u32
        }
        None => 10u32,
    };
    let converted = format_radix(value, base as u32);
    Ok(converted.into())
}

#[derive(Clone, Copy, Debug)]
pub struct FormatInt;

impl Function for FormatInt {
    fn identifier(&self) -> &'static str {
        "format_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::INTEGER,
                required: true,
            },
            Parameter {
                keyword: "base",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: (&mut state::LocalEnv, &mut state::ExternalEnv),
        _ctx: &mut FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let base = arguments.optional("base");

        Ok(Box::new(FormatIntFn { value, base }))
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "format decimal integer",
                source: r#"format_int!(42)"#,
                // extra "s are needed to avoid being read as an integer by tests
                result: Ok("\"42\""),
            },
            Example {
                title: "format hexadecimal integer",
                source: r#"format_int!(42, 16)"#,
                result: Ok("2a"),
            },
            Example {
                title: "format negative hexadecimal integer",
                source: r#"format_int!(-42, 16)"#,
                result: Ok("-2a"),
            },
        ]
    }

    fn call_by_vm(&self, _ctx: &mut Context, args: &mut VmArgumentList) -> Resolved {
        let value = args.required("value");
        let base = args.optional("base");

        format_int(value, base)
    }
}

#[derive(Clone, Debug)]
struct FormatIntFn {
    value: Box<dyn Expression>,
    base: Option<Box<dyn Expression>>,
}

impl Expression for FormatIntFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;

        let base = self
            .base
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;

        format_int(value, base)
    }

    fn type_def(&self, _: (&state::LocalEnv, &state::ExternalEnv)) -> TypeDef {
        TypeDef::integer().fallible()
    }
}

// Formats x in the provided radix
//
// Panics if radix is < 2 or > 36
fn format_radix(x: i64, radix: u32) -> String {
    let mut result: VecDeque<char> = VecDeque::new();

    let (mut x, negative) = if x < 0 {
        (-x as u64, true)
    } else {
        (x as u64, false)
    };

    loop {
        let m = (x % radix as u64) as u32; // max of 35
        x /= radix as u64;

        result.push_front(std::char::from_digit(m, radix).unwrap());
        if x == 0 {
            break;
        }
    }

    if negative {
        result.push_front('-');
    }

    result.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        format_int => FormatInt;

        decimal {
            args: func_args![value: 42],
            want: Ok(value!("42")),
            tdef: TypeDef::integer().fallible(),
        }

        hexidecimal {
            args: func_args![value: 42, base: 16],
            want: Ok(value!("2a")),
            tdef: TypeDef::integer().fallible(),
        }

        negative_hexidecimal {
            args: func_args![value: -42, base: 16],
            want: Ok(value!("-2a")),
            tdef: TypeDef::integer().fallible(),
        }
    ];
}
