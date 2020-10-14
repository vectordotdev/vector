use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct Sha1Fn {
    query: Box<dyn Function>,
}

impl Sha1Fn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for Sha1Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        use sha1::{Digest, Sha1};

        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => {
                let sha1 = hex::encode(Sha1::digest(&bytes));
                Ok(Value::Bytes(sha1.into()))
            }
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for Sha1Fn {
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
    fn sha1() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from("0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33")),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'boolean'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Boolean(true));

        let _ = Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
    }
}
