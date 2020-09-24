use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct Md5Fn {
    query: Box<dyn Function>,
}

impl Md5Fn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for Md5Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        use md5::{Digest, Md5};

        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => {
                let md5 = hex::encode(Md5::digest(&bytes));
                Ok(Value::Bytes(md5.into()))
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

impl TryFrom<ArgumentList> for Md5Fn {
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
    fn md5() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from("acbd18db4cc2f85cedef654fccc4a4d8")),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
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

        let _ = Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
    }
}
