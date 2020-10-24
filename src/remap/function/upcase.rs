use remap::prelude::*;
use std::convert::TryFrom;

#[derive(Debug)]
pub struct Upcase;

impl Function for Upcase {
    fn identifier(&self) -> &'static str {
        "upcase"
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

        Ok(Box::new(UpcaseFn { value }))
    }
}

#[derive(Debug)]
struct UpcaseFn {
    value: Box<dyn Expression>,
}

impl UpcaseFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>) -> Self {
        Self { value }
    }
}

impl Expression for UpcaseFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        self.value
            .execute(state, object)?
            .map(String::try_from)
            .transpose()?
            .map(|v| v.to_uppercase())
            .map(Into::into)
            .map(Ok)
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn upcase() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                UpcaseFn::new(Box::new(Path::from("foo"))),
            ),
            (
                map!["foo": "foo 2 bar"],
                Ok(Some(Value::from("FOO 2 BAR"))),
                UpcaseFn::new(Box::new(Path::from("foo"))),
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
