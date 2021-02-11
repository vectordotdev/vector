use chrono::{TimeZone, Utc};
use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ToTimestamp;

impl Function for ToTimestamp {
    fn identifier(&self) -> &'static str {
        "to_timestamp"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| {
                matches!(
                    v,
                    Value::Integer(_) |
                    Value::Float(_) |
                    Value::Bytes(_) |
                    Value::Timestamp(_)
                )
            },
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToTimestampFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToTimestampFn {
    value: Box<dyn Expression>,
}

impl ToTimestampFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ToTimestampFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let value = self.value.execute(state, object)?;

        match value {
            Timestamp(_) => Ok(value),
            Integer(v) => Ok(Timestamp(Utc.timestamp(v, 0))),
            Float(v) => Ok(Timestamp(Utc.timestamp(v.round() as i64, 0))),
            Bytes(v) => Conversion::Timestamp
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Boolean(_) | Array(_) | Map(_) | Regex(_) | Null => {
                Err("unable to convert value to timestamp".into())
            }
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(Kind::Timestamp | Kind::Integer | Kind::Float)
            .with_constraint(Kind::Timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    remap::test_type_def![
        timestamp_infallible {
            expr: |_| ToTimestampFn { value: Literal::from(chrono::Utc::now()).boxed() },
            def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToTimestampFn { value: lit!(1).boxed() },
            def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToTimestampFn { value: lit!(1.0).boxed() },
            def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
        }

        null_fallible {
            expr: |_| ToTimestampFn { value: lit!(null).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Timestamp,
                ..Default::default()
            },
        }

        string_fallible {
            expr: |_| ToTimestampFn { value: lit!("foo").boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Timestamp,
                ..Default::default()
            },
        }

        map_fallible {
            expr: |_| ToTimestampFn { value: map!{}.boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Timestamp,
                ..Default::default()
            },
        }

        array_fallible {
            expr: |_| ToTimestampFn { value: array![].boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Timestamp,
                ..Default::default()
            },
        }

        boolean_fallible {
            expr: |_| ToTimestampFn { value: lit!(true).boxed() },
            def: TypeDef {
                fallible: true,
                kind: Kind::Timestamp,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn to_timestamp() {
        use shared::btreemap;

        let cases = vec![(
            btreemap! { "foo" => Utc.timestamp(10, 0) },
            Ok(Value::Timestamp(Utc.timestamp(10, 0))),
            ToTimestampFn::new(Box::new(Path::from("foo"))),
        )];

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
