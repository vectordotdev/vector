use remap::prelude::*;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct ToUnixTimestamp;

impl Function for ToUnixTimestamp {
    fn identifier(&self) -> &'static str {
        "to_unix_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "unit",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        let unit = arguments
            .optional_enum("unit", &Unit::all_str())?
            .map(|s| Unit::from_str(&s).expect("validated enum"))
            .unwrap_or_default();

        Ok(Box::new(ToUnixTimestampFn { value, unit }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Unit {
    Seconds,
    Milliseconds,
    Nanoseconds,
}

impl Unit {
    fn all_str() -> Vec<&'static str> {
        use Unit::*;

        vec![Seconds, Milliseconds, Nanoseconds]
            .into_iter()
            .map(|u| u.as_str())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Unit::*;

        match self {
            Seconds => "seconds",
            Milliseconds => "milliseconds",
            Nanoseconds => "nanoseconds",
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Unit::Seconds
    }
}

impl FromStr for Unit {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Unit::*;

        match s {
            "seconds" => Ok(Seconds),
            "milliseconds" => Ok(Milliseconds),
            "nanoseconds" => Ok(Nanoseconds),
            _ => Err("unit not recognized"),
        }
    }
}

#[derive(Clone, Debug)]
struct ToUnixTimestampFn {
    value: Box<dyn Expression>,
    unit: Unit,
}

impl ToUnixTimestampFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, unit: Unit) -> Self {
        Self { value, unit }
    }
}

impl Expression for ToUnixTimestampFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let ts = self.value.execute(state, object)?.try_timestamp()?;

        let time = match self.unit {
            Unit::Seconds => ts.timestamp(),
            Unit::Milliseconds => ts.timestamp_millis(),
            Unit::Nanoseconds => ts.timestamp_nanos(),
        };

        Ok(time.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Timestamp)
            .with_constraint(value::Kind::Integer)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::TimeZone;
    use shared::btreemap;
    use value::Kind;

    test_type_def![
        timestamp_infallible {
            expr: |_| ToUnixTimestampFn {
                value: Literal::from(chrono::Utc::now()).boxed(),
                unit: Unit::Seconds,
            },
            def: TypeDef {
                kind: Kind::Integer,
                ..Default::default()
            },
        }

        string_fallible {
            expr: |_| ToUnixTimestampFn {
                value: lit!("late December back in '63").boxed(),
                unit: Unit::Seconds,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Integer,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_unix_timestamp() {
        let cases = vec![
            (
                btreemap! {},
                Ok(1609459200.into()),
                ToUnixTimestampFn::new(
                    Literal::from(chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)).boxed(),
                    Unit::Seconds,
                ),
            ),
            (
                btreemap! {},
                Ok(1609459200000i64.into()),
                ToUnixTimestampFn::new(
                    Literal::from(chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)).boxed(),
                    Unit::Milliseconds,
                ),
            ),
            (
                btreemap! {},
                Ok(1609459200000000000i64.into()),
                ToUnixTimestampFn::new(
                    Literal::from(chrono::Utc.ymd(2021, 1, 1).and_hms_milli(0, 0, 0, 0)).boxed(),
                    Unit::Nanoseconds,
                ),
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
