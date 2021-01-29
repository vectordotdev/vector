use remap::prelude::*;
use rust_decimal::{prelude::FromPrimitive, Decimal};

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

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let scale = arguments.optional("scale").map(Expr::boxed);
        let decimal_separator = arguments.optional("decimal_separator").map(Expr::boxed);
        let grouping_separator = arguments.optional("grouping_separator").map(Expr::boxed);

        Ok(Box::new(FormatNumberFn {
            value,
            scale,
            decimal_separator,
            grouping_separator,
        }))
    }
}

#[derive(Debug, Clone)]
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
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value: Decimal = match self.value.execute(state, object)? {
            Value::Integer(v) => v.into(),
            Value::Float(v) => Decimal::from_f64(v).expect("not NaN"),
            _ => unreachable!(),
        };

        let scale = match &self.scale {
            Some(expr) => Some(expr.execute(state, object)?.try_integer()?),
            None => None,
        };

        let grouping_separator = match &self.grouping_separator {
            Some(expr) => Some(expr.execute(state, object)?.try_bytes()?),
            None => None,
        };

        let decimal_separator = match &self.decimal_separator {
            Some(expr) => expr.execute(state, object)?.try_bytes()?,
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

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        let scale_def = self
            .scale
            .as_ref()
            .map(|scale| scale.type_def(state).fallible_unless(Kind::Integer));

        let decimal_separator_def = self.decimal_separator.as_ref().map(|decimal_separator| {
            decimal_separator
                .type_def(state)
                .fallible_unless(Kind::Bytes)
        });

        let grouping_separator_def = self.grouping_separator.as_ref().map(|grouping_separator| {
            grouping_separator
                .type_def(state)
                .fallible_unless(Kind::Bytes)
        });

        self.value
            .type_def(state)
            .fallible_unless(Kind::Integer | Kind::Float)
            .merge_optional(scale_def)
            .merge_optional(decimal_separator_def)
            .merge_optional(grouping_separator_def)
            .into_fallible(true) // `Decimal::from` can theoretically fail.
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_integer {
            expr: |_| FormatNumberFn {
                value: Literal::from(1).boxed(),
                scale: None,
                decimal_separator: None,
                grouping_separator: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_float {
            expr: |_| FormatNumberFn {
                value: Literal::from(1.0).boxed(),
                scale: None,
                decimal_separator: None,
                grouping_separator: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        // TODO(jean): we should update the function to ignore `None` values,
        // instead of aborting.
        optional_scale {
            expr: |_| FormatNumberFn {
                value: Literal::from(1.0).boxed(),
                scale: Some(Box::new(Noop)),
                decimal_separator: None,
                grouping_separator: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn format_number() {
        let cases = vec![
            (
                btreemap! {},
                Ok("1234.567".into()),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), None, None, None),
            ),
            (
                btreemap! {},
                Ok("1234.56".into()),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), Some(2), None, None),
            ),
            (
                btreemap! {},
                Ok("1234,56".into()),
                FormatNumberFn::new(Box::new(Literal::from(1234.567)), Some(2), Some(","), None),
            ),
            (
                btreemap! {},
                Ok("1 234,56".into()),
                FormatNumberFn::new(
                    Box::new(Literal::from(1234.567)),
                    Some(2),
                    Some(","),
                    Some(" "),
                ),
            ),
            (
                btreemap! {},
                Ok("11.222.333.444,567".into()),
                FormatNumberFn::new(
                    Box::new(Literal::from(11222333444.56789)),
                    Some(3),
                    Some(","),
                    Some("."),
                ),
            ),
            (
                btreemap! {},
                Ok("100".into()),
                FormatNumberFn::new(Box::new(Literal::from(100.0)), None, None, None),
            ),
            (
                btreemap! {},
                Ok("100.00".into()),
                FormatNumberFn::new(Box::new(Literal::from(100.0)), Some(2), None, None),
            ),
            (
                btreemap! {},
                Ok("123".into()),
                FormatNumberFn::new(Box::new(Literal::from(123.45)), Some(0), None, None),
            ),
            (
                btreemap! {},
                Ok("12345.00".into()),
                FormatNumberFn::new(Box::new(Literal::from(12345)), Some(2), None, None),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
