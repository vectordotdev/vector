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
            kind: kind::ANY,
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value")?;

        Ok(Box::new(ToTimestampFn { value }))
    }
}

#[derive(Debug)]
struct ToTimestampFn {
    value: Box<dyn Expression>,
}

impl Expression for ToTimestampFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        use Value::*;

        let value = match self.value.resolve(ctx)? {
            v @ Timestamp(_) => v,
            Integer(v) => Utc.timestamp(v, 0).into(),
            Float(v) => Utc.timestamp(v.round() as i64, 0).into(),
            Bytes(v) => Conversion::Timestamp
                .convert::<Value>(v)
                .map_err(|err| err.to_string())?
                .into(),
            v => Err(format!("unable to coerce {} into timestamp", v.kind()))?,
        };

        Ok(value)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(Kind::Timestamp | Kind::Integer | Kind::Float)
            .timestamp()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use value::Kind;

//     remap::test_type_def![
//         timestamp_infallible {
//             expr: |_| ToTimestampFn { value: Literal::from(chrono::Utc::now()).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         integer_infallible {
//             expr: |_| ToTimestampFn { value: lit!(1).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         float_infallible {
//             expr: |_| ToTimestampFn { value: lit!(1.0).boxed() },
//             def: TypeDef { kind: Kind::Timestamp, ..Default::default() },
//         }

//         null_fallible {
//             expr: |_| ToTimestampFn { value: lit!(null).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         string_fallible {
//             expr: |_| ToTimestampFn { value: lit!("foo").boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         map_fallible {
//             expr: |_| ToTimestampFn { value: map!{}.boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         array_fallible {
//             expr: |_| ToTimestampFn { value: array![].boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }

//         boolean_fallible {
//             expr: |_| ToTimestampFn { value: lit!(true).boxed() },
//             def: TypeDef {
//                 fallible: true,
//                 kind: Kind::Timestamp,
//                 ..Default::default()
//             },
//         }
//     ];

//     #[test]
//     fn to_timestamp() {
//         use crate::map;

//         let cases = vec![(
//             map!["foo": Utc.timestamp(10, 0)],
//             Ok(Value::Timestamp(Utc.timestamp(10, 0))),
//             ToTimestampFn::new(Box::new(Path::from("foo"))),
//         )];

//         let mut state = state::Program::default();

//         for (object, exp, func) in cases {
//             let mut object: Value = object.into();
//             let got = func
//                 .execute(&mut state, &mut object)
//                 .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

//             assert_eq!(got, exp);
//         }
//     }
// }
