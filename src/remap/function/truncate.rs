use remap::prelude::*;

#[derive(Debug)]
pub struct Truncate;

impl Function for Truncate {
    fn identifier(&self) -> &'static str {
        "truncate"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_)),
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let limit = arguments.required_expr("limit")?;
        let ellipsis = arguments.optional_expr("ellipsis")?;

        Ok(Box::new(TruncateFn {
            value,
            limit,
            ellipsis,
        }))
    }
}

#[derive(Debug)]
struct TruncateFn {
    value: Box<dyn Expression>,
    limit: Box<dyn Expression>,
    ellipsis: Option<Box<dyn Expression>>,
}

impl TruncateFn {
    #[cfg(test)]
    fn new(
        value: Box<dyn Expression>,
        limit: Box<dyn Expression>,
        ellipsis: Option<Value>,
    ) -> Self {
        let ellipsis = ellipsis.map(|b| Box::new(Literal::from(b)) as _);

        Self {
            value,
            limit,
            ellipsis,
        }
    }
}

impl Expression for TruncateFn {
    fn execute(&self, state: &mut State, object: &mut dyn Object) -> Result<Option<Value>> {
        let mut value = {
            let bytes = required!(state, object, self.value, Value::String(v) => v);
            String::from_utf8_lossy(&bytes).into_owned()
        };

        let limit = required!(
            state, object, self.limit,
            Value::Float(f) if f >= 0.0 => f.floor() as usize,
            Value::Integer(i) if i >= 0 => i as usize,
        );

        let ellipsis =
            optional!(state, object, self.ellipsis, Value::Boolean(v) => v).unwrap_or_default();

        let pos = if let Some((pos, chr)) = value.char_indices().take(limit).last() {
            // char_indices gives us the starting position of the character at limit,
            // we want the end position.
            pos + chr.len_utf8()
        } else {
            // We have an empty string
            0
        };

        if value.len() > pos {
            value.truncate(pos);

            if ellipsis {
                value.push_str("...");
            }
        }

        Ok(Some(value.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;

    #[test]
    fn truncate() {
        let cases = vec![
            (
                map!["foo": "Super"],
                Ok(Some("".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(0.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok(Some("...".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(0.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok(Some("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(10.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok(Some("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok(Some("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "♔♕♖♗♘♙♚♛♜♝♞♟"],
                Ok(Some("♔♕♖♗♘♙...".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(6.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok(Some("Super...".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok(Some("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    None,
                ),
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
