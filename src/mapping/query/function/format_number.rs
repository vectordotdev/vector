use super::prelude::*;
use bytes::Bytes;
use rust_decimal::{prelude::FromPrimitive, Decimal};

#[derive(Debug)]
pub(in crate::mapping) struct FormatNumberFn {
    query: Box<dyn Function>,
    scale: Option<Box<dyn Function>>,
    decimal_separator: Option<Box<dyn Function>>,
    grouping_separator: Option<Box<dyn Function>>,
}

impl FormatNumberFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        scale: Option<usize>,
        decimal_separator: Option<&str>,
        grouping_separator: Option<&str>,
    ) -> Self {
        let scale = scale.map(|v| Box::new(Literal::from(Value::from(v as i64))) as _);
        let decimal_separator =
            decimal_separator.map(|v| Box::new(Literal::from(Value::from(v))) as _);
        let grouping_separator =
            grouping_separator.map(|v| Box::new(Literal::from(Value::from(v))) as _);

        Self {
            query,
            scale,
            grouping_separator,
            decimal_separator,
        }
    }
}

impl Function for FormatNumberFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = required!(ctx, self.query,
            Value::Integer(v) => Decimal::from_i64(v),
            Value::Float(v) => Decimal::from_f64(v),
        )
        .ok_or("unable to parse number")?;

        let scale = optional!(ctx, self.scale, Value::Integer(v) => v);
        let grouping_separator = optional!(ctx, self.grouping_separator, Value::Bytes(v) => v);
        let decimal_separator = optional!(ctx, self.decimal_separator, Value::Bytes(v) => v)
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
        Ok(Value::from(
            parts.join(&String::from_utf8_lossy(&decimal_separator[..])),
        ))
    }

    fn parameters() -> &'static [Parameter] {
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
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
            Parameter {
                keyword: "grouping_separator",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for FormatNumberFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let scale = arguments.optional("scale");
        let decimal_separator = arguments.optional("decimal_separator");
        let grouping_separator = arguments.optional("grouping_separator");

        Ok(Self {
            query,
            scale,
            decimal_separator,
            grouping_separator,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn format_number() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                FormatNumberFn::new(Box::new(Path::from(vec![vec!["foo"]])), None, None, None),
            ),
            (
                Event::from(""),
                Ok(Value::from("1234.567")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(1234.567))),
                    None,
                    None,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("1234.56")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(1234.567))),
                    Some(2),
                    None,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("1234,56")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(1234.567))),
                    Some(2),
                    Some(","),
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("1 234,56")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(1234.567))),
                    Some(2),
                    Some(","),
                    Some(" "),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("11.222.333.444,567")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(11222333444.56789))),
                    Some(3),
                    Some(","),
                    Some("."),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("100")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(100.0))),
                    None,
                    None,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("100.00")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(100.0))),
                    Some(2),
                    None,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("123")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(123.45))),
                    Some(0),
                    None,
                    None,
                ),
            ),
            (
                Event::from(""),
                Ok(Value::from("12345.00")),
                FormatNumberFn::new(
                    Box::new(Literal::from(Value::from(12345))),
                    Some(2),
                    None,
                    None,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
