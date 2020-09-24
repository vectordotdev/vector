use super::prelude::*;
use bytes::Bytes;

#[derive(Debug)]
pub(in crate::mapping) struct UuidV4Fn {}

impl UuidV4Fn {
    #[cfg(test)]
    pub(in crate::mapping) fn new() -> Self {
        Self {}
    }
}

impl Function for UuidV4Fn {
    fn execute(&self, _: &Event) -> Result<Value> {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Value::Bytes(Bytes::copy_from_slice(uuid.as_bytes())))
    }
}

impl TryFrom<ArgumentList> for UuidV4Fn {
    type Error = String;

    fn try_from(_: ArgumentList) -> Result<Self> {
        Ok(Self {})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_v4() {
        match UuidV4Fn::new().execute(&Event::from("")).unwrap() {
            Value::Bytes(value) => {
                uuid::Uuid::parse_str(std::str::from_utf8(&value).unwrap()).expect("valid UUID V4")
            }
            _ => panic!("unexpected uuid_v4 output"),
        };
    }
}
