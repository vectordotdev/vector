use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ContainsFn {
    query: Box<dyn Function>,
    substring: Box<dyn Function>,
    case_sensitive: Option<Box<dyn Function>>,
}

impl ContainsFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        substring: &str,
        case_sensitive: bool,
    ) -> Self {
        let substring = Box::new(Literal::from(Value::from(substring)));
        let case_sensitive = Some(Box::new(Literal::from(Value::from(case_sensitive))) as _);

        Self {
            query,
            substring,
            case_sensitive,
        }
    }
}

impl Function for ContainsFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let substring = {
            let bytes = required!(ctx, self.substring, Value::Bytes(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let value = {
            let bytes = required!(ctx, self.query, Value::Bytes(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let contains = value.contains(&substring)
            || optional!(ctx, self.case_sensitive, Value::Boolean(b) => b)
                .iter()
                .filter(|&case_sensitive| !case_sensitive)
                .any(|_| value.to_lowercase().contains(&substring.to_lowercase()));

        Ok(Value::from(contains))
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "substring",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "case_sensitive",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ContainsFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let substring = arguments.required("substring")?;
        let case_sensitive = arguments.optional("case_sensitive");

        Ok(Self {
            query,
            substring,
            case_sensitive,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn contains() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ContainsFn::new(Box::new(Path::from(vec![vec!["foo"]])), "", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(false)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foo"))), "bar", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(false)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foo"))), "foobar", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foo"))), "foo", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foobar"))), "oba", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foobar"))), "foo", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foobar"))), "bar", false),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("fooBAR"))), "BAR", true),
            ),
            (
                Event::from(""),
                Ok(Value::from(false)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foobar"))), "BAR", true),
            ),
            (
                Event::from(""),
                Ok(Value::from(true)),
                ContainsFn::new(Box::new(Literal::from(Value::from("foobar"))), "BAR", false),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
