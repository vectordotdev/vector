use ::value::Value;
use rust_decimal::{prelude::FromPrimitive, Decimal};
use vrl::prelude::*;

fn format_number(
    value: Value,
    scale: Option<Value>,
    grouping_separator: Option<Value>,
    decimal_separator: Option<Value>,
) -> Resolved {
    let value: Decimal = match value {
        Value::Integer(v) => v.into(),
        Value::Float(v) => Decimal::from_f64(*v).expect("not NaN"),
        value => {
            return Err(value::Error::Expected {
                got: value.kind(),
                expected: Kind::integer() | Kind::float(),
            }
            .into())
        }
    };
    let scale = match scale {
        Some(expr) => Some(expr.try_integer()?),
        None => None,
    };
    let grouping_separator = match grouping_separator {
        Some(expr) => Some(expr.try_bytes()?),
        None => None,
    };
    let decimal_separator = match decimal_separator {
        Some(expr) => expr.try_bytes()?,
        None => ".".into(),
    };
    // Split integral and fractional part of float.
    let mut parts = value
        .to_string()
        .split('.')
        .map(ToOwned::to_owned)
        .collect::<Vec<String>>();
    debug_assert!(parts.len() <= 2);
    // Manipulate fractional part based on configuration.
    match scale {
        Some(i) if i == 0 => parts.truncate(1),
        Some(i) => {
            let i = i as usize;

            if parts.len() == 1 {
                parts.push(String::new())
            }

            if i > parts[1].len() {
                for _ in 0..i - parts[1].len() {
                    parts[1].push('0')
                }
            } else {
                parts[1].truncate(i)
            }
        }
        None => {}
    }
    // Manipulate integral part based on configuration.
    if let Some(sep) = grouping_separator.as_deref() {
        let sep = String::from_utf8_lossy(sep);
        let start = parts[0].len() % 3;

        let positions: Vec<usize> = parts[0]
            .chars()
            .skip(start)
            .enumerate()
            .map(|(i, _)| i)
            .filter(|i| i % 3 == 0)
            .collect();

        for (i, pos) in positions.iter().enumerate() {
            parts[0].insert_str(pos + (i * sep.len()) + start, &sep);
        }
    }
    // Join results, using configured decimal separator.
    Ok(parts
        .join(&String::from_utf8_lossy(&decimal_separator[..]))
        .into())
}

#[derive(Clone, Copy, Debug)]
pub struct FormatNumber;

impl Function for FormatNumber {
    fn identifier(&self) -> &'static str {
        "format_number"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::INTEGER | kind::FLOAT,
                required: true,
            },
            Parameter {
                keyword: "scale",
                kind: kind::INTEGER,
                required: false,
            },
            Parameter {
                keyword: "decimal_separator",
                kind: kind::BYTES,
                required: false,
            },
            Parameter {
                keyword: "grouping_separator",
                kind: kind::BYTES,
                required: false,
            },
        ]
    }

    fn compile(
        &self,
        _state: &state::TypeState,
        _ctx: &mut FunctionCompileContext,
        arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let scale = arguments.optional("scale");
        let decimal_separator = arguments.optional("decimal_separator");
        let grouping_separator = arguments.optional("grouping_separator");

        Ok(FormatNumberFn {
            value,
            scale,
            decimal_separator,
            grouping_separator,
        }
        .as_expr())
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "format number",
            source: r#"format_number(4672.4, decimal_separator: ",", grouping_separator: "_")"#,
            result: Ok("4_672,4"),
        }]
    }
}

#[derive(Clone, Debug)]
struct FormatNumberFn {
    value: Box<dyn Expression>,
    scale: Option<Box<dyn Expression>>,
    decimal_separator: Option<Box<dyn Expression>>,
    grouping_separator: Option<Box<dyn Expression>>,
}

impl FunctionExpression for FormatNumberFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let scale = self
            .scale
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;
        let grouping_separator = self
            .grouping_separator
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;
        let decimal_separator = self
            .decimal_separator
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?;

        format_number(value, scale, grouping_separator, decimal_separator)
    }

    fn type_def(&self, _: &state::TypeState) -> TypeDef {
        TypeDef::bytes().infallible()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    test_function![
        format_number => FormatNumber;

        number {
            args: func_args![value: 1234.567],
            want: Ok(value!("1234.567")),
            tdef: TypeDef::bytes().infallible(),
        }

        precision {
            args: func_args![value: 1234.567,
                             scale: 2],
            want: Ok(value!("1234.56")),
            tdef: TypeDef::bytes().infallible(),
        }


        separator {
            args: func_args![value: 1234.567,
                             scale: 2,
                             decimal_separator: ","],
            want: Ok(value!("1234,56")),
            tdef: TypeDef::bytes().infallible(),
        }

        more_separators {
            args: func_args![value: 1234.567,
                             scale: 2,
                             decimal_separator: ",",
                             grouping_separator: " "],
            want: Ok(value!("1 234,56")),
            tdef: TypeDef::bytes().infallible(),
        }

        big_number {
            args: func_args![value: 11_222_333_444.567_89,
                             scale: 3,
                             decimal_separator: ",",
                             grouping_separator: "."],
            want: Ok(value!("11.222.333.444,567")),
            tdef: TypeDef::bytes().infallible(),
        }

        integer {
            args: func_args![value: 100.0],
            want: Ok(value!("100")),
            tdef: TypeDef::bytes().infallible(),
        }

        integer_decimals {
            args: func_args![value: 100.0,
                             scale: 2],
            want: Ok(value!("100.00")),
            tdef: TypeDef::bytes().infallible(),
        }

        float_no_decimals {
            args: func_args![value: 123.45,
                             scale: 0],
            want: Ok(value!("123")),
            tdef: TypeDef::bytes().infallible(),
        }

        integer_no_decimals {
            args: func_args![value: 12345,
                             scale: 2],
            want: Ok(value!("12345.00")),
            tdef: TypeDef::bytes().infallible(),
        }
    ];
}
