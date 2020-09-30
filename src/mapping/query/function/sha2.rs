use super::prelude::*;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512Trunc224, Sha512Trunc256};

#[derive(Debug)]
pub(in crate::mapping) struct Sha2Fn {
    query: Box<dyn Function>,
    variant: Option<Box<dyn Function>>,
}

impl Sha2Fn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, variant: Option<&str>) -> Self {
        let variant = variant.map(|v| Box::new(Literal::from(Value::from(v))) as _);

        Self { query, variant }
    }
}

impl Function for Sha2Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = required!(ctx, self.query, Value::Bytes(v) => v);
        let variant = optional!(ctx, self.variant, Value::Bytes(v) => v);

        let hash = match variant.as_deref() {
            Some(b"SHA-224") => encode::<Sha224>(&value),
            Some(b"SHA-256") => encode::<Sha256>(&value),
            Some(b"SHA-384") => encode::<Sha384>(&value),
            Some(b"SHA-512") => encode::<Sha512>(&value),
            Some(b"SHA-512/224") => encode::<Sha512Trunc224>(&value),
            Some(b"SHA-512/256") | None => encode::<Sha512Trunc256>(&value),
            Some(v) => {
                return Err(format!(
                    "unknown SHA-2 algorithm variant: '{}'",
                    String::from_utf8_lossy(v)
                ))
            }
        };

        Ok(Value::Bytes(hash.into()))
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "variant",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: false,
            },
        ]
    }
}

#[inline(always)]
fn encode<T: Digest>(value: &[u8]) -> String {
    hex::encode(T::digest(value))
}

impl TryFrom<ArgumentList> for Sha2Fn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let variant = arguments.optional("variant");

        Ok(Self { query, variant })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn sha2() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_owned()),
                Sha2Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Err("unknown SHA-2 algorithm variant: 'bar'".to_owned()),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("bar")),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from(
                    "d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d",
                )),
                Sha2Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "0808f64e60d58979fcb676c96ec938270dea42445aeefcd3a4e6f8db",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-224")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-256")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "98c11ffdfdd540676b1a137cb1a22b2a70350c9a44171d6b1180c6be5cbb2ee3f79d532c8a1dd9ef2e8e08e752a3babb",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-384")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "f7fbba6e0636f890e56fbbf3283e524c6fa3204ae298382d624741d0dc6638326e282c41be5e4254d8820772c5518a2c5a8c0c7f7eda19594a7eb539453e1ed7",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-512")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-512/224")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d",
                )),
                Sha2Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA-512/256")),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'boolean'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Boolean(true));

        let _ = Sha2Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None).execute(&event);
    }
}
