use regex::Regex;
use remap::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Match;

impl Function for Match {
    fn identifier(&self) -> &'static str {
        "match"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |_| true,
                required: true,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let pattern = arguments.required_regex("pattern")?;

        Ok(Box::new(MatchFn { value, pattern }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MatchFn {
    value: Box<dyn Expression>,
    pattern: Regex,
}

impl MatchFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, pattern: Regex) -> Self {
        Self { value, pattern }
    }
}

impl Expression for MatchFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        required!(
            state, object, self.value,

            Value::String(b) => {
                let value = String::from_utf8_lossy(&b);
                Ok(Some(self.pattern.is_match(&value).into()))
            }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn r#match() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                MatchFn::new(Box::new(Path::from("foo")), Regex::new("").unwrap()),
            ),
            (
                map!["foo": "foobar"],
                Ok(Some(false.into())),
                MatchFn::new(Box::new(Path::from("foo")), Regex::new("\\s\\w+").unwrap()),
            ),
            (
                map!["foo": "foo 2 bar"],
                Ok(Some(true.into())),
                MatchFn::new(
                    Box::new(Path::from("foo")),
                    Regex::new("foo \\d bar").unwrap(),
                ),
            ),
            // `Noop` returns `Ok(None)`, which is passed-through
            (
                map![],
                Ok(None),
                MatchFn::new(Box::new(Noop), Regex::new("true").unwrap()),
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
