use remap::prelude::*;
use sha_3::{Digest, Sha3_224, Sha3_256, Sha3_384, Sha3_512};

const VARIANTS: &[&str] = &["SHA3-224", "SHA3-256", "SHA3-384", "SHA3-512"];

#[derive(Clone, Copy, Debug)]
pub struct Sha3;

impl Function for Sha3 {
    fn identifier(&self) -> &'static str {
        "sha3"
    }

    fn parameters(&self) -> &'static [Parameter] {
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

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required("value")?.boxed();
        let variant = arguments.optional_enum("variant", &VARIANTS)?;

        Ok(Box::new(Sha3Fn { value, variant }))
    }
}

#[derive(Debug, Clone)]
struct Sha3Fn {
    value: Box<dyn Expression>,
    variant: Option<String>,
}

impl Sha3Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, variant: Option<&str>) -> Self {
        let variant = variant.map(|v| v.to_owned());

        Self { value, variant }
    }
}

impl Expression for Sha3Fn {
    fn execute(&self, state: &mut state::Program, object: &mut dyn Object) -> Result<Value> {
        let value = self.value.execute(state, object)?.try_bytes()?;

        let hash = match self.variant.as_deref() {
            Some("SHA3-224") => encode::<Sha3_224>(&value),
            Some("SHA3-256") => encode::<Sha3_256>(&value),
            Some("SHA3-384") => encode::<Sha3_384>(&value),
            Some("SHA3-512") | None => encode::<Sha3_512>(&value),
            _ => unreachable!("enum invariant"),
        };

        Ok(hash.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.value
            .type_def(state)
            .fallible_unless(value::Kind::Bytes)
            .with_constraint(value::Kind::Bytes)
    }
}

#[inline]
fn encode<T: Digest>(value: &[u8]) -> String {
    hex::encode(T::digest(value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;
    use value::Kind;

    remap::test_type_def![
        value_string {
            expr: |_| Sha3Fn {
                value: Literal::from("foo").boxed(),
                variant: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string {
            expr: |_| Sha3Fn {
                value: Literal::from(1).boxed(),
                variant: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }

        value_optional {
            expr: |_| Sha3Fn {
                value: Box::new(Noop),
                variant: None,
            },
            def: TypeDef { fallible: true, kind: Kind::Bytes, ..Default::default() },
        }
    ];

    #[test]
    fn sha3() {
        let cases = vec![
            (
                btreemap!{ "foo" => "foo" },
                Ok("4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7".into()),
                Sha3Fn::new(Box::new(Path::from("foo")), None),
            ),
            (
                btreemap!{},
                Ok("f4f6779e153c391bbd29c95e72b0708e39d9166c7cea51d1f10ef58a".into()),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-224")),
            ),
            (
                btreemap!{},
                Ok("76d3bc41c9f588f7fcd0d5bf4718f8f84b1c41b20882703100b9eb9413807c01".into()),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-256")),
            ),
            (
                btreemap!{},
                Ok("665551928d13b7d84ee02734502b018d896a0fb87eed5adb4c87ba91bbd6489410e11b0fbcc06ed7d0ebad559e5d3bb5".into()),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-384")),
            ),
            (
                btreemap!{},
                Ok("4bca2b137edc580fe50a88983ef860ebaca36c857b1f492839d6d7392452a63c82cbebc68e3b70a2a1480b4bb5d437a7cba6ecf9d89f9ff3ccd14cd6146ea7e7".into()),
                Sha3Fn::new(Box::new(Literal::from("foo")), Some("SHA3-512")),
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
