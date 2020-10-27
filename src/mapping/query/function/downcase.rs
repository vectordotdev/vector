use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct DowncaseFn {
    query: Box<dyn Function>,
}

impl DowncaseFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for DowncaseFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let bytes = required_value!(ctx, self.query, Value::Bytes(v) => v);
        Ok(QueryValue::from_value(
            String::from_utf8_lossy(&bytes).to_lowercase(),
        ))
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for DowncaseFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn downcase() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("FOO 2 bar"));
                    event
                },
                Ok(QueryValue::from_value("foo 2 bar")),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'integer'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Integer(20));

        let _ = DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
    }
}
