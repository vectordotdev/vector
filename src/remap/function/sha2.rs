use remap::prelude::*;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512Trunc224, Sha512Trunc256};

#[derive(Debug)]
pub struct Sha2;

impl Function for Sha2 {
    fn identifier(&self) -> &'static str {
        "sha2"
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

        Ok(Box::new(Sha2Fn { value, variant }))
    }
}

#[derive(Debug)]
struct Sha2Fn {
    value: Box<dyn Expression>,
    variant: Option<Box<dyn Expression>>,
}

impl Sha2Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, variant: Option<&str>) -> Self {
        let variant = variant.map(|v| Box::new(Literal::from(v)) as _);

        Self { value, variant }
    }
}

impl Expression for Sha2Fn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(v) => v);
        let variant = optional!(state, object, self.variant, Value::String(v) => v);

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
    fn sha2() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                Sha2Fn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Err("function call error: unknown SHA-2 algorithm variant: 'bar'".into()),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("bar")),
            ),
            (
                map!["foo": "foo"],
                Ok(Some(
                    "d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d".into()
                )),
                Sha2Fn::new(Box::new(Path::from("foo")), None),
            ),
            (
                map![],
                Ok(Some(
                    "0808f64e60d58979fcb676c96ec938270dea42445aeefcd3a4e6f8db".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-224")),
            ),
            (
                map![],
                Ok(Some(
                    "2c26b46b68ffc68ff99b453c1d30413413422d706483bfa0f98a5e886266e7ae".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-256")),
            ),
            (
                map![],
                Ok(Some(
                    "98c11ffdfdd540676b1a137cb1a22b2a70350c9a44171d6b1180c6be5cbb2ee3f79d532c8a1dd9ef2e8e08e752a3babb".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-384")),
            ),
            (
                map![],
                Ok(Some(
                    "f7fbba6e0636f890e56fbbf3283e524c6fa3204ae298382d624741d0dc6638326e282c41be5e4254d8820772c5518a2c5a8c0c7f7eda19594a7eb539453e1ed7".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512")),
            ),
            (
                map![],
                Ok(Some(
                    "d68f258d37d670cfc1ec1001a0394784233f88f056994f9a7e5e99be".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512/224")),
            ),
            (
                map![],
                Ok(Some(
                    "d58042e6aa5a335e03ad576c6a9e43b41591bfd2077f72dec9df7930e492055d".into()
                )),
                Sha2Fn::new(Box::new(Literal::from("foo")), Some("SHA-512/256")),
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
