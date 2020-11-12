use remap::prelude::*;
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
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "pattern",
                accepts: |v| matches!(v, Value::String(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, Value::Integer(_)),
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Result<Box<dyn Expression>> {
        let value = arguments.required_expr("value")?;
        let pattern = arguments.required("pattern")?;
        let limit = arguments.optional_expr("limit")?;

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
    pattern: Argument,
    limit: Option<Box<dyn Expression>>,
}

impl Expression for SplitFn {
    fn execute(
        &self,
        state: &mut state::Program,
        object: &mut dyn Object,
    ) -> Result<Option<Value>> {
        let value = required!(state, object, self.value, Value::String(b) => String::from_utf8_lossy(&b).into_owned());
        let limit: usize = self
            .limit
            .as_ref()
            .and_then(|expr| expr.execute(state, object).transpose())
            .transpose()?
            .map(i64::try_from)
            .transpose()?
            .and_then(|i| usize::try_from(i).ok())
            .unwrap_or(usize::MAX);

        let value = match &self.pattern {
            Argument::Regex(pattern) => pattern
                .splitn(&value, limit as usize)
                .collect::<Vec<_>>()
                .into(),
            Argument::Expression(expr) => {
                let pattern = required!(state, object, expr, Value::String(b) => String::from_utf8_lossy(&b).into_owned());

                value.splitn(limit, &pattern).collect::<Vec<_>>().into()
            }
        };

        Ok(Some(value))
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        use value::Kind::*;

        let limit_def = self
            .limit
            .as_ref()
            .map(|limit| limit.type_def(state).fallible_unless(vec![Integer, Float]));

        let pattern_def = match &self.pattern {
            Argument::Expression(expr) => Some(expr.type_def(state).fallible_unless(String)),
            Argument::Regex(_) => None, // regex is a concrete infallible type
        };

        self.value
            .type_def(state)
            .fallible_unless(String)
            .merge_optional(limit_def)
            .merge_optional(pattern_def)
            .with_constraint(Array)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    remap::test_type_def![
        infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                limit: None,
            },
            def: TypeDef {
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }

        value_fallible {
            expr: |_| SplitFn {
                value: Literal::from(10).boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                limit: None,
            },
            def: TypeDef {
                fallible: true,
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }

        pattern_expression_infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from("foo").into(),
                limit: None,
            },
            def: TypeDef {
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }

        pattern_expression_fallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: Literal::from(10).into(),
                limit: None,
            },
            def: TypeDef {
                fallible: true,
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }

        limit_infallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                limit: Some(Literal::from(10).boxed()),
            },
            def: TypeDef {
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }

        limit_fallible {
            expr: |_| SplitFn {
                value: Literal::from("foo").boxed(),
                pattern: regex::Regex::new("foo").unwrap().into(),
                limit: Some(Literal::from("foo").boxed()),
            },
            def: TypeDef {
                fallible: true,
                constraint: value::Kind::Array.into(),
                ..Default::default()
            },
        }
    ];
}
