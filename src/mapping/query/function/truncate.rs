use super::prelude::*;

#[derive(Debug)]
pub(in crate::mapping) struct TruncateFn {
    query: Box<dyn Function>,
    limit: Box<dyn Function>,
    ellipsis: Option<Box<dyn Function>>,
}

impl TruncateFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        limit: Box<dyn Function>,
        ellipsis: Option<Value>,
    ) -> Self {
        let ellipsis = ellipsis.map(|b| Box::new(Literal::from(b)) as _);

        Self {
            query,
            limit,
            ellipsis,
        }
    }
}

impl Function for TruncateFn {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
        let bytes = required_value!(ctx, self.query, Value::Bytes(v) => v);

        let limit = required_value!(ctx, self.limit,
                                    Value::Float(f) if f >= 0.0 => f.floor() as usize,
                                    Value::Integer(i) if i >= 0 => i as usize,
        );

        let ellipsis = optional_value!(ctx, self.ellipsis,
                                 Value::Boolean(value) => value)
        .unwrap_or_default();

        if let Ok(s) = std::str::from_utf8(&bytes) {
            let pos = if let Some((pos, chr)) = s.char_indices().take(limit).last() {
                // char_indices gives us the starting position of the character at limit,
                // we want the end position.
                pos + chr.len_utf8()
            } else {
                // We have an empty string
                0
            };

            if s.len() <= pos {
                // No truncating necessary.
                Ok(Value::Bytes(bytes).into())
            } else if ellipsis {
                // Allocate a new string to add the ellipsis to.
                let mut new = s[0..pos].to_string();
                new.push_str("...");
                Ok(Value::Bytes(new.into()).into())
            } else {
                // Just pull the relevant part out of the original parameter.
                Ok(Value::Bytes(bytes.slice(0..pos)).into())
            }
        } else {
            // Not a valid utf8 string.
            Err("unable to truncate from non-unicode string types".to_string())
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, QueryValue::Value(Value::Bytes(_))),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| {
                    matches!(v, QueryValue::Value(Value::Integer(_))
                                      | QueryValue::Value(Value::Float(_)))
                },
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                accepts: |v| matches!(v, QueryValue::Value(Value::Boolean(_))),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for TruncateFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let limit = arguments.required("limit")?;
        let ellipsis = arguments.optional("ellipsis");

        Ok(Self {
            query,
            limit,
            ellipsis,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;

    #[test]
    fn truncate() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(0.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(0.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(10.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("Supercalifragilisticexpialidocious"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("♔♕♖♗♘♙♚♛♜♝♞♟"));
                    event
                },
                Ok(Value::Bytes("♔♕♖♗♘♙...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(6.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("Supercalifragilisticexpialidocious"));
                    event
                },
                Ok(Value::Bytes("Super...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("Supercalifragilisticexpialidocious"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    None,
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp.map(QueryValue::Value));
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'float'")]
    fn negative_value() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::from("Super"));
        let _ = TruncateFn::new(
            Box::new(Path::from(vec![vec!["foo"]])),
            Box::new(Literal::from(Value::Float(-5.0))),
            Some(Value::Boolean(true)),
        )
        .execute(&event);
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'float'")]
    fn invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Float(3.0));
        let _ = TruncateFn::new(
            Box::new(Path::from(vec![vec!["foo"]])),
            Box::new(Literal::from(Value::Float(5.0))),
            Some(Value::Boolean(true)),
        )
        .execute(&event);
    }
}
