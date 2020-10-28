use remap::prelude::*;

#[derive(Debug)]
pub struct EndsWith;

impl Function for EndsWith {
    fn identifier(&self) -> &'static str {
        "ends_with"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "substring",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let substring = arguments.required_expr("substring")?;
        let case_sensitive = arguments.optional_expr("case_sensitive")?;

        Ok(Box::new(EndsWithFn {
            value,
            substring,
            case_sensitive,
        }))
    }
}

#[derive(Debug)]
struct EndsWithFn {
    value: Box<dyn Expression>,
    substring: Box<dyn Expression>,
    case_sensitive: Option<Box<dyn Expression>>,
}

impl EndsWithFn {
    #[cfg(test)]
    fn new(value: Box<dyn Expression>, substring: &str, case_sensitive: bool) -> Self {
        let substring = Box::new(Literal::from(substring));
        let case_sensitive = Some(Box::new(Literal::from(case_sensitive)) as _);

        Self {
            value,
            substring,
            case_sensitive,
        }
    }
}

impl Expression for EndsWithFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let substring = {
            let bytes = required!(state, object, self.substring, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let starts_with = value.ends_with(&substring)
            || optional!(state, object, self.case_sensitive, Value::Boolean(b) => b)
                .iter()
                .filter(|&case_sensitive| !case_sensitive)
                .any(|_| value.to_lowercase().ends_with(&substring.to_lowercase()));

        Ok(Some(starts_with.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn ends_with() {
        let cases = vec![
            (
                map![],
                Err("path error: missing path: foo".into()),
                EndsWithFn::new(Box::new(Path::from("foo")), "", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foo", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "foobar", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("bar")), "bar", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "oba", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "bar", false),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "foo", false),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("fooBAR")), "BAR", true),
            ),
            (
                map![],
                Ok(Some(false.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", true),
            ),
            (
                map![],
                Ok(Some(true.into())),
                EndsWithFn::new(Box::new(Literal::from("foobar")), "BAR", false),
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
