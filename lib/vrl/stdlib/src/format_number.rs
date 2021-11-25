use rust_decimal::{prelude::FromPrimitive, Decimal};
use vrl::prelude::*;

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
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let value = arguments.required("value");
        let scale = arguments.optional("scale");
        let decimal_separator = arguments.optional("decimal_separator");
        let grouping_separator = arguments.optional("grouping_separator");

        Ok(Box::new(FormatNumberFn {
            value,
            scale,
            decimal_separator,
            grouping_separator,
        }))
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

impl Expression for FormatNumberFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let value = value.borrow();
        let value: Decimal = match &*value {
            Value::Integer(v) => (*v).into(),
            Value::Float(v) => Decimal::from_f64(**v).expect("not NaN"),
            value => {
                return Err(value::Error::Expected {
                    got: value.kind(),
                    expected: Kind::Integer | Kind::Float,
                }
                .into())
            }
        };

        let scale = match &self.scale {
            Some(expr) => Some(expr.resolve(ctx)?.try_integer()?),
            None => None,
        };

        let grouping_separator = match &self.grouping_separator {
            Some(expr) => {
                let separator = expr.resolve(ctx)?;
                let separator = separator.borrow();
                Some(separator.try_bytes()?)
            }
            None => None,
        };

        let decimal_separator = match &self.decimal_separator {
            Some(expr) => {
                let separator = expr.resolve(ctx)?;
                let separator = separator.borrow();
                separator.try_bytes()?
            }
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
                    parts.push("".to_owned())
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

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
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
            tdef: TypeDef::new().infallible().bytes(),
        }

        precision {
            args: func_args![value: 1234.567,
                             scale: 2],
            want: Ok(value!("1234.56")),
            tdef: TypeDef::new().infallible().bytes(),
        }


        separator {
            args: func_args![value: 1234.567,
                             scale: 2,
                             decimal_separator: ","],
            want: Ok(value!("1234,56")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        more_separators {
            args: func_args![value: 1234.567,
                             scale: 2,
                             decimal_separator: ",",
                             grouping_separator: " "],
            want: Ok(value!("1 234,56")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        big_number {
            args: func_args![value: 11222333444.56789,
                             scale: 3,
                             decimal_separator: ",",
                             grouping_separator: "."],
            want: Ok(value!("11.222.333.444,567")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        integer {
            args: func_args![value: 100.0],
            want: Ok(value!("100")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        integer_decimals {
            args: func_args![value: 100.0,
                             scale: 2],
            want: Ok(value!("100.00")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        float_no_decimals {
            args: func_args![value: 123.45,
                             scale: 0],
            want: Ok(value!("123")),
            tdef: TypeDef::new().infallible().bytes(),
        }

        integer_no_decimals {
            args: func_args![value: 12345,
                             scale: 2],
            want: Ok(value!("12345.00")),
            tdef: TypeDef::new().infallible().bytes(),
        }
    ];
}
