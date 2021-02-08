use remap::prelude::*;
use shared::conversion::Conversion;

#[derive(Clone, Copy, Debug)]
pub struct ToInt;

impl Function for ToInt {
    fn identifier(&self) -> &'static str {
        "to_int"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_) | Value::Bytes(_) | Value::Boolean(_) | Value::Timestamp(_) | Value::Null),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();

        Ok(Box::new(ToIntFn { value }))
    }
}

#[derive(Debug, Clone)]
struct ToIntFn {
    value: Box<dyn Expression>,
}

impl ToIntFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for ToIntFn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        use Value::*;

        let value = self.value.execute(state, object)?;

        match value {
            Integer(_) => Ok(value),
            Float(v) => Ok(Integer(v as i64)),
            Boolean(v) => Ok(Integer(if v { 1 } else { 0 })),
            Null => Ok(0.into()),
            Bytes(v) => Conversion::Integer
                .convert(v)
                .map_err(|e| e.to_string().into()),
            Timestamp(v) => Ok(v.timestamp().into()),
            Array(_) | Map(_) | Regex(_) => Err("unable to convert value to integer".into()),
        }
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind;

        self.value
            .type_def(state)
            .fallible_unless(
                Kind::Integer
                    | Kind::Float
                    | Kind::Bytes
                    | Kind::Boolean
                    | Kind::Timestamp
                    | Kind::Null,
            )
            .with_constraint(Kind::Integer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use value::Kind;

    remap::test_type_def![
        boolean_infallible {
            expr: |_| ToIntFn { value: lit!(true).boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        integer_infallible {
            expr: |_| ToIntFn { value: lit!(1).boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        float_infallible {
            expr: |_| ToIntFn { value: lit!(1.0).boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        null_infallible {
            expr: |_| ToIntFn { value: lit!(null).boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        string_fallible {
            expr: |_| ToIntFn { value: lit!("foo").boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }

        map_fallible {
            expr: |_| ToIntFn { value: map!{}.boxed() },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        array_fallible {
            expr: |_| ToIntFn { value: array![].boxed() },
            def: TypeDef { fallible: true, kind: Kind::Integer, ..Default::default() },
        }

        timestamp_infallible {
            expr: |_| ToIntFn { value: Literal::from(chrono::Utc::now()).boxed() },
            def: TypeDef { kind: Kind::Integer, ..Default::default() },
        }
    ];

    #[test]
    fn to_int() {
        use shared::btreemap;

        let cases = vec![
            (
                btreemap! { "foo" => "20" },
                Ok(Value::Integer(20)),
                ToIntFn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! { "foo" => 20.5 },
                Ok(Value::Integer(20)),
                ToIntFn::new(Box::new(Path::from("foo"))),
            ),
            (
                btreemap! {
                    "foo" => DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                              .unwrap()
                              .with_timezone(&Utc),
                },
                Ok(Value::Integer(1571227200)),
                ToIntFn::new(Box::new(Path::from("foo"))),
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
