use remap::prelude::*;
use sha3::{Digest, Sha3_224, Sha3_256, Sha3_384, Sha3_512};

#[derive(Debug)]
pub struct Sha3;

impl Function for Sha3 {
    fn identifier(&self) -> &'static str {
        "sha3"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::String(_)),
            required: true,
        }]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let variant = arguments.optional_expr("variant")?;

        Ok(Box::new(Sha3Fn { value, variant }))
    }
}

#[derive(Debug)]
struct Sha3Fn {
    value: Box<dyn Expression>,
    variant: Option<Box<dyn Expression>>,
}

impl Sha3Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, variant: Option<&str>) -> Self {
        let variant = variant.map(|v| Box::new(Literal::from(v)) as _);

        Self { value, variant }
    }
}

impl Expression for Sha3Fn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(v) => v);
        let variant = optional!(state, object, self.variant, Value::String(v) => v);

        let hash = match variant.as_deref() {
            Some(b"SHA3-224") => encode::<Sha3_224>(&value),
            Some(b"SHA3-256") => encode::<Sha3_256>(&value),
            Some(b"SHA3-384") => encode::<Sha3_384>(&value),
            Some(b"SHA3-512") | None => encode::<Sha3_512>(&value),
            Some(v) => {
                return Err(format!(
                    "unknown SHA-3 algorithm variant: '{}'",
                    String::from_utf8_lossy(v)
                )
                .into())
            }
        };

        Ok(Some(hash.into()))
    }
}

#[inline]
fn encode<T: Digest>(value: &[u8]) -> String {
    hex::encode(T::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn sha3() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                Sha3Fn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Err("function call error: unknown SHA-3 algorithm variant: 'bar'".into()),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("bar")),
            ),
            (
                map!["foo": "foo"],
                Ok(Some(
                    "4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7".into()
                )),
                Sha3Fn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(
                    "f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a".into()
                )),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-224")),
            ),
            (
                map![],
                Ok(Some(
                    "76d3bc41c9f588f7fcd0d5bf4718f8f84b1c41b20882703100b9eb9413807c01".into()
                )),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-256")),
            ),
            (
                map![],
                Ok(Some(
                    "665551928d13b7d84ee02734502b018d896a0fb87eed5adb4c87ba91bbd6489410e11b0fbcc06ed7d0ebad559e5d3bb5".into()
                )),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-384")),
            ),
            (
                map![],
                Ok(Some(
                    "4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7".into()
                )),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-512")),
            ),
        ];

        let mut state = remap::State::default();

        for (mut object, exp, func) in cases {
            let got = func
                .execute(&mut state, &mut object)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
