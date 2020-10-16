use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct ParseJsonFn {
    query: Box<dyn Function>,
}

impl ParseJsonFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        ParseJsonFn { query }
    }
}

impl Function for ParseJsonFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        match self.query.execute(ctx)? {
            QueryValue::Value(Value::Bytes(b)) => serde_json::from_slice(&b)
                .map(|v: serde_json::Value| {
                    let v: Value = v.into();
                    v.into()
                })
                .map_err(|err| format!("unable to parse json {}", err)),
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for ParseJsonFn {
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
    use std::collections::BTreeMap;

    #[test]
    fn parse_json() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("42"));
                    event
                },
                Ok(Value::from(42)),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("\"hello\""));
                    event
                },
                Ok(Value::from("hello")),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("{\"field\": \"value\"}"));
                    event
                },
                Ok(Value::from({
                    let mut map = BTreeMap::new();
                    map.insert("field".into(), Value::from("value"));
                    map
                })),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("{\"field\"x \"value\"}"));
                    event
                },
                Err("unable to parse json expected `:` at line 1 column 9".into()),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
