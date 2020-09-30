use super::prelude::*;
use lazy_static::lazy_static;
use regex::Regex;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::HashMap;
use std::str::FromStr;

lazy_static! {
    static ref RE: Regex = Regex::new(
        r"(?ix)                        # i: case-insensitive, x: ignore whitespace + comments
            \A
            (?P<value>[0-9]*\.?[0-9]+) # value: integer or float
            \s?                        # optional space between value and unit
            (?P<unit>[a-z]{1,2})       # unit: one or two letters
            \z"
    )
    .unwrap();
    static ref UNITS: HashMap<String, Decimal> = vec![
        ("ns", Decimal::new(1, 9)),
        ("us", Decimal::new(1, 6)),
        ("µs", Decimal::new(1, 6)),
        ("ms", Decimal::new(1, 3)),
        ("cs", Decimal::new(1, 2)),
        ("ds", Decimal::new(1, 1)),
        ("s", Decimal::new(1, 0)),
        ("m", Decimal::new(60, 0)),
        ("h", Decimal::new(3_600, 0)),
        ("d", Decimal::new(86_400, 0)),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect();
}

#[derive(Debug)]
pub(in crate::mapping) struct ParseDurationFn {
    query: Box<dyn Function>,
    output: Box<dyn Function>,
}

impl ParseDurationFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, output: &str) -> Self {
        let output = Box::new(Literal::from(Value::from(output)));

        Self { query, output }
    }
}

impl Function for ParseDurationFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = {
            let bytes = required!(ctx, self.query, Value::Bytes(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let conversion_factor = {
            let bytes = required!(ctx, self.output, Value::Bytes(v) => v);
            let output = String::from_utf8_lossy(&bytes).into_owned();

            UNITS
                .get(&output)
                .ok_or(format!("unknown output format: '{}'", output))?
        };

        let captures = RE
            .captures(&value)
            .ok_or(format!("unable to parse duration: '{}'", value))?;

        let value = Decimal::from_str(&captures["value"])
            .map_err(|e| format!("unable to parse number: {}", e))?;

        let unit = UNITS
            .get(&captures["unit"])
            .ok_or(format!("unknown duration unit: '{}'", &captures["unit"]))?;

        let number = value * unit / conversion_factor;
        let number = number
            .to_f64()
            .ok_or(format!("unable to format duration: '{}'", number))?;

        Ok(Value::from(number))
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "output",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ParseDurationFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let output = arguments.required("output")?;

        Ok(Self { query, output })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn parse_duration() {
        let cases = vec![
            (
                Event::from(""),
                Ok(Value::from(0.5)),
                ParseDurationFn::new(Box::new(Literal::from("30s")), "m"),
            ),
            (
                Event::from(""),
                Ok(Value::from(1.2)),
                ParseDurationFn::new(Box::new(Literal::from("1200ms")), "s"),
            ),
            (
                Event::from(""),
                Ok(Value::from(100.0)),
                ParseDurationFn::new(Box::new(Literal::from("100ms")), "ms"),
            ),
            (
                Event::from(""),
                Ok(Value::from(1.005)),
                ParseDurationFn::new(Box::new(Literal::from("1005ms")), "s"),
            ),
            (
                Event::from(""),
                Ok(Value::from(0.0001)),
                ParseDurationFn::new(Box::new(Literal::from("100ns")), "ms"),
            ),
            (
                Event::from(""),
                Ok(Value::from(86400.0)),
                ParseDurationFn::new(Box::new(Literal::from("1d")), "s"),
            ),
            (
                Event::from(""),
                Ok(Value::from(1000000000.0)),
                ParseDurationFn::new(Box::new(Literal::from("1 s")), "ns"),
            ),
            (
                Event::from(""),
                Err("path .foo not found in event".to_owned()),
                ParseDurationFn::new(Box::new(Path::from(vec![vec!["foo"]])), "s"),
            ),
            (
                Event::from(""),
                Err("unable to parse duration: 'foo'".to_owned()),
                ParseDurationFn::new(Box::new(Literal::from("foo")), "µs"),
            ),
            (
                Event::from(""),
                Err("unable to parse duration: '1'".to_owned()),
                ParseDurationFn::new(Box::new(Literal::from("1")), "ns"),
            ),
            (
                Event::from(""),
                Err("unknown duration unit: 'w'".to_owned()),
                ParseDurationFn::new(Box::new(Literal::from("1w")), "ns"),
            ),
            (
                Event::from(""),
                Err("unknown output format: 'w'".to_owned()),
                ParseDurationFn::new(Box::new(Literal::from("1s")), "w"),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
