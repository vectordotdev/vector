use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct StripWhitespaceFn {
    query: Box<dyn Function>,
}

impl StripWhitespaceFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for StripWhitespaceFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        match self.query.execute(ctx)? {
            QueryValue::Value(Value::Bytes(b)) => std::str::from_utf8(&b)
                .map(|s| Value::Bytes(b.slice_ref(s.trim().as_bytes())))
                .map(Into::into)
                .map_err(|_| {
                    "unable to strip white_space from non-unicode string types".to_owned()
                }),
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

impl TryFrom<ArgumentList> for StripWhitespaceFn {
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
    fn strip_whitespace() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from(""));
                    event
                },
                Ok(Value::Bytes("".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("    "));
                    event
                },
                Ok(Value::Bytes("".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("hi there"));
                    event
                },
                Ok(Value::Bytes("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("     hi there    "));
                    event
                },
                Ok(Value::Bytes("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(" \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "),
                    );
                    event
                },
                Ok(Value::Bytes("❤❤ hi there ❤❤".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }
}
