use bytes::Bytes;
use remap::prelude::*;

#[derive(Debug)]
pub struct UuidV4;

impl Function for UuidV4 {
    fn identifier(&self) -> &'static str {
        "uuid_v4"
    }

    fn compile(&self, _: ArgumentList) -> Result<Box<dyn Expression>> {
        Ok(Box::new(UuidV4Fn))
    }
}

#[derive(Debug)]
struct UuidV4Fn;

impl Expression for UuidV4Fn {
    fn execute(&self, _: &mut State, _: &mut dyn Object) -> Result<Option<Value>> {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Some(Bytes::copy_from_slice(uuid.as_bytes()).into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    use std::convert::TryFrom;

    #[test]
    fn uuid_v4() {
        let mut state = remap::State::default();
        let mut object = map![];
        let value = UuidV4Fn.execute(&mut state, &mut object).unwrap().unwrap();

        assert!(matches!(&value, Value::String(_)));

        uuid::Uuid::parse_str(&String::try_from(value).unwrap()).expect("valid UUID V4");
    }
}
