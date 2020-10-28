use remap::prelude::*;

#[derive(Debug)]
pub struct Sha1;

impl Function for Sha1 {
    fn identifier(&self) -> &'static str {
        "sha1"
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

        Ok(Box::new(Sha1Fn { value }))
    }
}

#[derive(Debug)]
struct Sha1Fn {
    value: Box<dyn Expression>,
}

impl Sha1Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for Sha1Fn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use ::sha1::{Digest, Sha1};

        self.value.execute(state, object).map(|r| {
            r.map(|v| match v.as_string_lossy() {
                Value::String(bytes) => Value::String(hex::encode(Sha1::digest(&bytes)).into()),
                _ => unreachable!(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn sha1() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                Sha1Fn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "foo"],
                Ok(Some(Value::from(
                    "0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33",
                ))),
                Sha1Fn::new(Box::new(Path::from("foo"))),
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
