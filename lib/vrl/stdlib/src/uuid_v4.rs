use bytes::Bytes;
use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct UuidV4;

impl Function for UuidV4 {
    fn identifier(&self) -> &'static str {
        "uuid_v4"
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "generate UUID v4",
            source: r#"uuid_v4() != """#,
            result: Ok("true"),
        }]
    }

    fn compile(&self, _: ArgumentList) -> Compiled {
        Ok(Box::new(UuidV4Fn))
    }
}

#[derive(Debug, Clone, Copy)]
struct UuidV4Fn;

impl Expression for UuidV4Fn {
    fn resolve(&self, _: &mut Context) -> Resolved {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Bytes::copy_from_slice(uuid.as_bytes()).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef::new().infallible().bytes()
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::map;
//     use std::convert::TryFrom;

//     vrl::test_type_def![static_def {
//         expr: |_| UuidV4Fn,
//         def: TypeDef {
//             kind: value::Kind::Bytes,
//             ..Default::default()
//         },
//     }];

//     #[test]
//     fn uuid_v4() {
//         let mut state = state::Program::default();
//         let mut object: Value = map![].into();
//         let value = UuidV4Fn.resolve(&mut ctx).unwrap();

//         assert!(matches!(&value, Value::Bytes(_)));

//         uuid::Uuid::parse_str(&String::try_from(value).unwrap()).expect("valid UUID V4");
//     }
// }
