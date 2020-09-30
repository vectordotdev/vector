use super::prelude::*;
use bytes::Bytes;

#[derive(Debug)]
pub(in crate::mapping) struct StripAnsiEscapeCodesFn {
    query: Box<dyn Function>,
}

impl StripAnsiEscapeCodesFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for StripAnsiEscapeCodesFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let bytes = required!(ctx, self.query, Value::Bytes(v) => v);

        strip_ansi_escapes::strip(&bytes)
            .map(Bytes::from)
            .map(Value::from)
            .map_err(|e| e.to_string())
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for StripAnsiEscapeCodesFn {
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
    fn contains() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                StripAnsiEscapeCodesFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                Event::from(""),
                Ok(Value::from("foo bar")),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from(Value::from("foo bar")))),
            ),
            (
                Event::from(""),
                Ok(Value::from("foo bar")),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from(Value::from(
                    "\x1b[3;4Hfoo bar",
                )))),
            ),
            (
                Event::from(""),
                Ok(Value::from("foo bar")),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from(Value::from(
                    "\x1b[46mfoo\x1b[0m bar",
                )))),
            ),
            (
                Event::from(""),
                Ok(Value::from("foo bar")),
                StripAnsiEscapeCodesFn::new(Box::new(Literal::from(Value::from(
                    "\x1b[=3lfoo bar",
                )))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
