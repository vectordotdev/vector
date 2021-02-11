use lazy_static::lazy_static;
use regex::Regex;
use rust_decimal::{prelude::ToPrimitive, Decimal};
use std::collections::HashMap;
use std::str::FromStr;
use vrl::prelude::*;

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

#[derive(Clone, Copy, Debug)]
pub struct ParseDuration;

impl Function for ParseDuration {
    fn identifier(&self) -> &'static str {
        "parse_duration"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "milliseconds",
            source: r#"parse_duration!("1005ms", unit: "s")"#,
            result: Ok("1.005"),
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let unit = arguments.required("unit");

        Ok(Box::new(ParseDurationFn { value, unit }))
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::BYTES,
                required: true,
            },
            Parameter {
                keyword: "unit",
                kind: kind::BYTES,
                required: true,
            },
        ]
    }
}

#[derive(Debug, Clone)]
struct ParseDurationFn {
    value: Box<dyn Expression>,
    unit: Box<dyn Expression>,
}

impl Expression for ParseDurationFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let bytes = self.value.resolve(ctx)?.unwrap_bytes();
        let value = String::from_utf8_lossy(&bytes);

        let conversion_factor = {
            let bytes = self.unit.resolve(ctx)?.unwrap_bytes();
            let string = String::from_utf8_lossy(&bytes);

            UNITS
                .get(string.as_ref())
                .ok_or(format!("unknown unit format: '{}'", string))?
        };

        let captures = RE
            .captures(&value)
            .ok_or(format!("unable to parse duration: '{}'", value))?;

        let value = Decimal::from_str(&captures["value"])
            .map_err(|error| format!("unable to parse number: {}", error))?;

        let unit = UNITS
            .get(&captures["unit"])
            .ok_or(format!("unknown duration unit: '{}'", &captures["unit"]))?;

        let number = value * unit / conversion_factor;
        let number = number
            .to_f64()
            .ok_or(format!("unable to format duration: '{}'", number))?;

        Ok(number.into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().fallible().float()
    }
}

#[cfg(test)]
mod tests {
    // use super::*;
    // use crate::map;

    // vrl::test_type_def![
    //     value_string {
    //         expr: |_| ParseDurationFn {
    //             value: Literal::from("foo").boxed(),
    //             unit: Literal::from("foo").boxed(),
    //         },
    //         def: TypeDef { fallible: true, kind: value::Kind::Float, ..Default::default() },
    //     }

    //     optional_expression {
    //         expr: |_| ParseDurationFn {
    //             value: Box::new(Noop),
    //             unit: Literal::from("foo").boxed(),
    //         },
    //         def: TypeDef { fallible: true, kind: value::Kind::Float, ..Default::default() },
    //     }
    // ];

    // #[test]
    // fn parse_duration() {
    //     let cases = vec![
    //         (
    //             map![],
    //             Ok(0.5.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("30s")), "m"),
    //         ),
    //         (
    //             map![],
    //             Ok(1.2.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1200ms")), "s"),
    //         ),
    //         (
    //             map![],
    //             Ok(100.0.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("100ms")), "ms"),
    //         ),
    //         (
    //             map![],
    //             Ok(1.005.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1005ms")), "s"),
    //         ),
    //         (
    //             map![],
    //             Ok(0.0001.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("100ns")), "ms"),
    //         ),
    //         (
    //             map![],
    //             Ok(86400.0.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1d")), "s"),
    //         ),
    //         (
    //             map![],
    //             Ok(1000000000.0.into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1 s")), "ns"),
    //         ),
    //         (
    //             map![],
    //             Err("function call error: unable to parse duration: 'foo'".into()),
    //             ParseDurationFn::new(Box::new(Literal::from("foo")), "µs"),
    //         ),
    //         (
    //             map![],
    //             Err("function call error: unable to parse duration: '1'".into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1")), "ns"),
    //         ),
    //         (
    //             map![],
    //             Err("function call error: unknown duration unit: 'w'".into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1w")), "ns"),
    //         ),
    //         (
    //             map![],
    //             Err("function call error: unknown unit format: 'w'".into()),
    //             ParseDurationFn::new(Box::new(Literal::from("1s")), "w"),
    //         ),
    //     ];

    //     let mut state = state::Program::default();

    //     for (object, exp, func) in cases {
    //         let mut object: Value = object.into();
    //         let got = func
    //             .resolve(&mut ctx)
    //             .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

    //         assert_eq!(got, exp);
    //     }
    // }
}
