use remap::prelude::*;

#[derive(Debug)]
pub struct Md5;

impl Function for Md5 {
    fn identifier(&self) -> &'static str {
        "md5"
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

        Ok(Box::new(Md5Fn { value }))
    }
}

#[derive(Debug)]
struct Md5Fn {
    value: Box<dyn Expression>,
}

impl Md5Fn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for Md5Fn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        use md5::{Digest, Md5};

        self.value.execute(state, object).map(|r| {
            r.map(|v| match v.as_string_lossy() {
                Value::String(bytes) => Value::String(hex::encode(Md5::digest(&bytes)).into()),
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
    fn md5() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                Md5Fn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "foo"],
                Ok(Some(Value::from("acbd18db4cc2f85cedef654fccc4a4d8"))),
                Md5Fn::new(Box::new(Path::from("foo"))),
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
