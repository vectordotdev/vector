use bytes::Bytes;
use remap::prelude::*;
use rust_decimal::{prelude::FromPrimitive, Decimal};

#[derive(Debug)]
pub struct FormatNumber;

impl Function for FormatNumber {
    fn identifier(&self) -> &'static str {
        "format_number"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_)),
                required: true,
            },
            Parameter {
                keyword: "scale",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
            Parameter {
                keyword: "decimal_separator",
                accepts: |v| matches!(v, Value::String(_)),
                required: false,
            },
            Parameter {
                keyword: "grouping_separator",
                accepts: |v| matches!(v, Value::String(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let scale = arguments.optional_expr("scale")?;
        let decimal_separator = arguments.optional_expr("decimal_separator")?;
        let grouping_separator = arguments.optional_expr("grouping_separator")?;

        Ok(Box::new(FormatNumberFn {
            value,
            scale,
            decimal_separator,
            grouping_separator,
        }))
    }
}

#[derive(Debug)]
struct FormatNumberFn {
    value: Box<dyn Expression>,
    scale: Option<Box<dyn Expression>>,
    decimal_separator: Option<Box<dyn Expression>>,
    grouping_separator: Option<Box<dyn Expression>>,
}

impl FormatNumberFn {
    #[cfg(test)]
    fn new(
        value: Box<dyn Expression>,
        scale: Option<usize>,
        decimal_separator: Option<&str>,
        grouping_separator: Option<&str>,
    ) -> Self {
        let scale = scale.map(|v| Box::new(Literal::from(v as i64)) as _);
        let decimal_separator = decimal_separator.map(|v| Box::new(Literal::from(v)) as _);
        let grouping_separator = grouping_separator.map(|v| Box::new(Literal::from(v)) as _);

        Self {
            value,
            scale,
            grouping_separator,
            decimal_separator,
        }
    }
}

impl Expression for FormatNumberFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = required!(state, object, self.value,
            Value::Integer(v) => Decimal::from_i64(v),
            Value::Float(v) => Decimal::from_f64(v),
        )
        .ok_or("unable to parse number")?;

        let scale = optional!(state, object, self.scale, Value::Integer(v) => v);
        let grouping_separator =
            optional!(state, object, self.grouping_separator, Value::String(v) => v);
        let decimal_separator =
            optional!(state, object, self.decimal_separator, Value::String(v) => v)
                .unwrap_or_else(|| Bytes::from("."));

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
                        parts[1].push_str("0")
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
        Ok(Some(
            parts
                .join(&String::from_utf8_lossy(&decimal_separator[..]))
                .into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn format_number() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                FormatNumberFn::new(Box::new(Path::from("foo")), None, None, None),
            ),
            (
                map![],
                Ok(Some("1234.567".into())),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), None, None, None),
            ),
            (
                map![],
                Ok(Some("1234.56".into())),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), Some(2), None, None),
            ),
            (
                map![],
                Ok(Some("1234,56".into())),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), Some(2), Some(","), None),
            ),
            (
                map![],
                Ok(Some("1 234,56".into())),
                FormatNumberFn::new(
                    Box::new(Literal::from(1234.567)),
                    Some(2),
                    Some(","),
                    Some(" "),
                ),
            ),
            (
                map![],
                Ok(Some("11.222.333.444,567".into())),
                FormatNumberFn::new(
                    Box::new(Literal::from(11222333444.56789)),
                    Some(3),
                    Some(","),
                    Some("."),
                ),
            ),
            (
                map![],
                Ok(Some("100".into())),
                FormatNumberFn::new(Box::new(Literal::from(100.0)), None, None, None),
            ),
            (
                map![],
                Ok(Some("100.00".into())),
                FormatNumberFn::new(Box::new(Literal::from(100.0)), Some(2), None, None),
            ),
            (
                map![],
                Ok(Some("123".into())),
                FormatNumberFn::new(Box::new(Literal::from(123.45)), Some(0), None, None),
            ),
            (
                map![],
                Ok(Some("12345.00".into())),
                FormatNumberFn::new(Box::new(Literal::from(12345)), Some(2), None, None),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
