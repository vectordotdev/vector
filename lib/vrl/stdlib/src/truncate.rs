use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Truncate;

impl Function for Truncate {
    fn identifier(&self) -> &'static str {
        "truncate"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                kind: kind::ANY,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let limit = arguments.required("limit");
        let ellipsis = arguments.optional("ellipsis");

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
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let mut value = self
            .value
            .resolve(ctx)?
            .try_bytes_utf8_lossy()?
            .into_owned();

        let limit = match self.limit.resolve(ctx)? {
            Value::Float(f) => f.floor() as i64,
            Value::Integer(i) => i,
            _ => unreachable!(),
        };

        let limit = if limit < 0 { 0 } else { limit as usize };
        let ellipsis = match &self.ellipsis {
            Some(expr) => expr.resolve(ctx)?.try_boolean()?,
            None => false,
        };

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

        Ok(value.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        
        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge(
                self.limit
                    .type_def(state)
                    .fallible_unless(Kind::Integer | Kind::Float),
            )
            .merge_optional(
                self.ellipsis
                    .as_ref()
                    .map(|ellipsis| ellipsis.type_def(state).fallible_unless(Kind::Boolean)),
            )
            .with_constraint(Kind::Bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map;
    
    vrl::test_type_def![
        infallible {
            expr: |_| TruncateFn {
                value: Literal::from("foo").boxed(),
                limit: Literal::from(1).boxed(),
                ellipsis: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        value_non_string {
            expr: |_| TruncateFn {
                value: Literal::from(false).boxed(),
                limit: Literal::from(1).boxed(),
                ellipsis: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        limit_float {
            expr: |_| TruncateFn {
                value: Literal::from("foo").boxed(),
                limit: Literal::from(1.0).boxed(),
                ellipsis: None,
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        limit_non_number {
            expr: |_| TruncateFn {
                value: Literal::from("foo").boxed(),
                limit: Literal::from("bar").boxed(),
                ellipsis: None,
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }

        ellipsis_boolean {
            expr: |_| TruncateFn {
                value: Literal::from("foo").boxed(),
                limit: Literal::from(10).boxed(),
                ellipsis: Some(Literal::from(true).boxed()),
            },
            def: TypeDef { kind: Kind::Bytes, ..Default::default() },
        }

        ellipsis_non_boolean {
            expr: |_| TruncateFn {
                value: Literal::from("foo").boxed(),
                limit: Literal::from("bar").boxed(),
                ellipsis: Some(Literal::from("baz").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: Kind::Bytes,
                ..Default::default()
            },
        }
    ];

    #[test]
    fn truncate() {
        let cases = vec![
            (
                map!["foo": "Super"],
                Ok("".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(0.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok("...".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(0.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok("Super".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(10.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "Super"],
                Ok("Super".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok("Super".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(false.into()),
                ),
            ),
            (
                map!["foo": "♔♕♖♗♘♙♚♛♜♝♞♟"],
                Ok("♔♕♖♗♘♙...".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(6.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok("Super...".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    Some(true.into()),
                ),
            ),
            (
                map!["foo": "Supercalifragilisticexpialidocious"],
                Ok("Super".into()),
                TruncateFn::new(
                    Box::new(Path::from("foo")),
                    Box::new(Literal::from(5.0)),
                    None,
                ),
            ),
        ];

        let mut state = state::Program::default();

        for (object, exp, func) in cases {
            let mut object: Value = object.into();
            let got = func
                .resolve(&mut ctx)
                .map_err(|e| format!("{:#}", anyhow::anyhow!(e)));

            assert_eq!(got, exp);
        }
    }
}
