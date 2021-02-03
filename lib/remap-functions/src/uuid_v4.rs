use bytes::Bytes;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct UuidV4;

impl Function for UuidV4 {
    fn identifier(&self) -> &'static str {
        "uuid_v4"
    }

    fn compile(&self, _: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(UuidV4Fn))
    }
}

#[derive(Debug, Clone)]
struct UuidV4Fn;

impl Expression for UuidV4Fn {
    fn execute(&self, _: &mut state::Program, _: &mut dyn Object) -> Result<Value> {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Bytes::copy_from_slice(uuid.as_bytes()).into())
    }

    fn type_def(&self, _: &state::Compiler) -> TypeDef {
        TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use std::convert::TryFrom;

    remap::test_type_def![static_def {
        expr: |_| UuidV4Fn,
        def: TypeDef {
            kind: value::Kind::Bytes,
            ..Default::default()
        },
    }];

    #[test]
    fn uuid_v4() {
        let mut state = state::Program::default();
        let mut object: Value = btreemap! {}.into();
        let value = UuidV4Fn.execute(&mut state, &mut object).unwrap();

        assert!(matches!(&value, Value::Bytes(_)));

        uuid::Uuid::parse_str(&String::try_from(value).unwrap()).expect("valid UUID V4");
    }
}
