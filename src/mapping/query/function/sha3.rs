use super::prelude::*;
use digest::DynDigest;
use sha3::{Digest, Sha3_224, Sha3_256, Sha3_384, Sha3_512};

#[derive(Debug)]
pub(in crate::mapping) struct Sha3Fn {
    query: Box<dyn Function>,
    variant: Option<Box<dyn Function>>,
}

impl Sha3Fn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, variant: Option<&str>) -> Self {
        let variant = variant.map(|v| Box::new(Literal::from(Value::from(v))) as _);

        Self { query, variant }
    }
}

impl Function for Sha3Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let bytes = required!(ctx, self.query, Value::Bytes(v) => v);

        let variant = optional!(ctx, self.variant, Value::Bytes(v) => v)
            .map(|v| String::from_utf8_lossy(&v).into_owned());

        let mut digest: Box<dyn DynDigest> = match variant.as_deref() {
            Some("SHA3-224") => Box::new(Sha3_224::new()),
            Some("SHA3-256") => Box::new(Sha3_256::new()),
            Some("SHA3-384") => Box::new(Sha3_384::new()),
            Some("SHA3-512") | None => Box::new(Sha3_512::new()),
            Some(v) => return Err(format!("unknown SHA-3 algorithm variant: '{}'", v)),
        };

        digest.update(&bytes);
        let sha3 = hex::encode(digest.finalize());

        Ok(Value::Bytes(sha3.into()))
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

impl TryFrom<ArgumentList> for Sha3Fn {
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
    fn sha3() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_owned()),
                Sha3Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Err("unknown SHA-3 algorithm variant: 'bar'".to_owned()),
                Sha3Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("bar")),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from(
                    "4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7",
                )),
                Sha3Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a",
                )),
                Sha3Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA3-224")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "76d3bc41c9f588f7fcd0d5bf4718f8f84b1c41b20882703100b9eb9413807c01",
                )),
                Sha3Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA3-256")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "665551928d13b7d84ee02734502b018d896a0fb87eed5adb4c87ba91bbd6489410e11b0fbcc06ed7d0ebad559e5d3bb5",
                )),
                Sha3Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA3-384")),
            ),
            (
                Event::from(""),
                Ok(Value::from(
                    "4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7",
                )),
                Sha3Fn::new(Box::new(Literal::from(Value::from("foo"))), Some("SHA3-512")),
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

        let _ = Sha3Fn::new(Box::new(Path::from(vec![vec!["foo"]])), None).execute(&event);
    }
}
