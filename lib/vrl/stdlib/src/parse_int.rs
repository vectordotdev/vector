use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct ParseInt;

impl Function for ParseInt {
    fn identifier(&self) -> &'static str {
        "parse_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "base",
                kind: kind::INTEGER,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[
            Example {
                title: "integer",
                source: r#"parse_int!("-42")"#,
                result: Ok("-42"),
            },
            Example {
                title: "hexidecimal",
                source: r#"parse_int!("0x2a")"#,
                result: Ok("42"),
            },
            Example {
                title: "hexidecimal explicit",
                source: r#"parse_int!("2a", base: 16)"#,
                result: Ok("42"),
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _info: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let base = arguments.optional("base");

        Ok(Box::new(ParseIntFn { value, base }))
    }
}

#[derive(Debug, Clone)]
struct ParseIntFn {
    value: Box<dyn Expression>,
    base: Option<Box<dyn Expression>>,
}

impl Expression for ParseIntFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;

        let (base, index) = match &self.base {
            Some(base) => base
                .resolve(ctx)
                .and_then(|value| value.try_integer().map_err(Into::into))
                .and_then(|value| {
                    if !(2..=36).contains(&value) {
                        return Err(format!(
                            "invalid base {}: must be be between 2 and 36 (inclusive)",
                            value
                        )
                        .into());
                    }

                    Ok((value as u32, 0))
                }),
            None => match string.chars().next() {
                Some('0') => match string.chars().nth(1) {
                    Some('b') => Ok((2, 2)),
                    Some('o') => Ok((8, 2)),
                    Some('x') => Ok((16, 2)),
                    _ => Ok((8, 1)),
                },
                Some(_) => Ok((10u32, 0)),
                None => Err("value is empty".into()),
            },
        }?;

        let converted = i64::from_str_radix(&string[index..], base)
            .map_err(|err| format!("could not parse integer: {}", err))?;

        Ok(converted.into())
    }

    fn type_def(&self, _state: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().integer()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        parse_int => ParseInt;

        decimal {
             args: func_args![value: "-42"],
             want: Ok(-42),
             tdef: TypeDef::new().fallible().integer(),
        }

        binary {
             args: func_args![value: "0b1001"],
             want: Ok(9),
             tdef: TypeDef::new().fallible().integer(),
        }

        octal {
             args: func_args![value: "042"],
             want: Ok(34),
             tdef: TypeDef::new().fallible().integer(),
        }

        hexidecimal {
             args: func_args![value: "0x2a"],
             want: Ok(42),
             tdef: TypeDef::new().fallible().integer(),
        }

        explicit_hexidecimal {
             args: func_args![value: "2a", base: 16],
             want: Ok(42),
             tdef: TypeDef::new().fallible().integer(),
        }
    ];
}
