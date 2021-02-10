use vrl::prelude::*;
use std::convert::TryFrom;

#[derive(Clone, Copy, Debug)]
pub struct Split;

impl Function for Split {
    fn identifier(&self) -> &'static str {
        "split"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "pattern",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "limit",
                kind: kind::ANY,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");
        let pattern = arguments.required("pattern");
        let limit = arguments.optional("limit");

        Ok(Box::new(SplitFn {
            value,
            pattern,
            limit,
        }))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SplitFn {
    value: Box<dyn Expression>,
    pattern: Box<dyn Expression>,
    limit: Option<Box<dyn Expression>>,
}

impl Expression for SplitFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let string = value.try_bytes_utf8_lossy()?;
        let limit: usize = self
            .limit
            .as_ref()
            .map(|expr| expr.resolve(ctx))
            .transpose()?
            .map(i64::try_from)
            .transpose()?
            .and_then(|i| usize::try_from(i).ok())
            .unwrap_or(usize::MAX);

        self.pattern
            .resolve(ctx)
            .and_then(|pattern| match pattern {
                Value::Regex(pattern) => Ok(pattern
                    .splitn(string.as_ref(), limit as usize)
                    .collect::<Vec<_>>()
                    .into()),
                Value::Bytes(bytes) => {
                    let pattern = String::from_utf8_lossy(&bytes);

                    Ok(string
                        .splitn(limit, pattern.as_ref())
                        .collect::<Vec<_>>()
                        .into())
                }
                v => Err(Error::Value(value::Error::Expected(
                    value::Kind::Bytes | value::Kind::Regex,
                    v.kind(),
                ))),
            })
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        
        let limit_def = self.limit.as_ref().map(|limit| {
            limit
                .type_def(state)
                .fallible_unless(Kind::Integer | Kind::Float)
        });

        let pattern_def = self
            .pattern
            .type_def(state)
            .fallible_unless(Kind::Bytes | Kind::Regex);

        self.value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .merge(pattern_def)
            .merge_optional(limit_def)
            .with_constraint(Kind::Array)
    }
}

#[cfg(test)]
#[allow(clippy::trivial_regex)]
mod test {
    use super::*;

    vrl::test_type_def![
        infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
                limit: None,
            },
            def: TypeDef {
                kind: value::Kind::Array,
                ..Default::default()
            },
        }

        value_fallible {
            expr: |_| SplitFn {
                value: Literal::from(10).boxed(),
                pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
                limit: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Array,
                ..Default::default()
            },
        }

        pattern_expression_infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from("foo").boxed(),
                limit: None,
            },
            def: TypeDef {
                kind: value::Kind::Array,
                ..Default::default()
            },
        }

        pattern_expression_fallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(10).boxed(),
                limit: None,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Array,
                ..Default::default()
            },
        }

        limit_infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
                limit: Some(Literal::from(10).boxed()),
            },
            def: TypeDef {
                kind: value::Kind::Array,
                ..Default::default()
            },
        }

        limit_fallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(regex::Regex::new("foo").unwrap()).boxed(),
                limit: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Array,
                ..Default::default()
            },
        }
    ];
}
