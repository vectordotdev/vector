/*
use vrl::prelude::*;
use std::str::FromStr;

#[derive(Clone, Copy, Debug)]
pub struct Redact;

impl Function for Redact {
    fn identifier(&self) -> &'static str {
        "redact"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                kind: kind::ANY,
                required: true,
            },
            Parameter {
                keyword: "filters",
                kind: kind::ANY,
                required: false,
            },
            Parameter {
                keyword: "redactor",
                kind: kind::ANY,
                required: false,
            },
            Parameter {
                keyword: "patterns",
                kind: kind::ANY,
                required: false,
            },
        ]
    }

    fn compile(&self, mut arguments: ArgumentList) -> Compiled {
        let value = arguments.required("value");

        let filters = arguments
            .optional_enum_list("filters", &Filter::all_str())?
            .unwrap_or_default()
            .into_iter()
            .map(|s| Filter::from_str(&s).expect("validated enum"))
            .collect::<Vec<_>>();

        let redactor = arguments
            .optional_enum("redactor", &Redactor::all_str())?
            .map(|s| Redactor::from_str(&s).expect("validated enum"))
            .unwrap_or_default();

        let patterns = arguments.optional_array("patterns")?.map(Into::into);

        Ok(Box::new(RedactFn {
            value,
            filters,
            redactor,
            patterns,
        }))
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug)]
struct RedactFn {
    value: Box<dyn Expression>,
    filters: Vec<Filter>,
    redactor: Redactor,
    patterns: Option<Vec<Expr>>,
}

impl Expression for RedactFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let value = self.value.resolve(ctx)?;
        let mut input = value.try_bytes_utf8_lossy()?.into_owned();

        for filter in &self.filters {
            match filter {
                Filter::Pattern => self
                    .patterns
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .try_for_each::<_, Result<()>>(|expr| match expr.resolve(ctx)? {
                        Value::Bytes(bytes) => {
                            let pattern = String::from_utf8_lossy(&bytes);

                            input = input.replace(pattern.as_ref(), self.redactor.pattern());
                            Ok(())
                        }
                        Value::Regex(regex) => {
                            input = regex
                                .replace_all(&input, self.redactor.pattern())
                                .into_owned();
                            Ok(())
                        }
                        v => Err(value::Error::Expected(
                            value::Kind::Bytes | value::Kind::Regex,
                            v.kind(),
                        )
                        .into()),
                    })?,
            }
        }

        Ok(input.into())
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        let mut typedef = self
            .value
            .type_def(state)
            .fallible_unless(Kind::Bytes)
            .with_constraint(Kind::Bytes);

        match &self.patterns {
            Some(patterns) => {
                for p in patterns {
                    typedef = typedef.merge(
                        p.type_def(state)
                            .fallible_unless(Kind::Regex)
                            .with_constraint(Kind::Bytes),
                    )
                }
            }
            None => (),
        }

        typedef
    }
}

// -----------------------------------------------------------------------------

/// The redaction filter to apply to the given value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Filter {
    Pattern,
}

impl Filter {
    fn all_str() -> Vec<&'static str> {
        use Filter::*;

        vec![Pattern]
            .into_iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
    }

    const fn as_str(self) -> &'static str {
        use Filter::*;

        match self {
            Pattern => "pattern",
        }
    }
}

impl FromStr for Filter {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Filter::*;

        match s {
            "pattern" => Ok(Pattern),
            _ => Err("unknown filter"),
        }
    }
}

// -----------------------------------------------------------------------------

/// The recipe for redacting the matched filters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Redactor {
    Full,
}

impl Redactor {
    fn all_str() -> Vec<&'static str> {
        use Redactor::*;

        vec![Full]
            .into_iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>()
    }

    fn as_str(self) -> &'static str {
        use Redactor::*;

        match self {
            Full => "full",
        }
    }

    fn pattern(&self) -> &str {
        use Redactor::*;

        match self {
            Full => "****",
        }
    }
}

impl Default for Redactor {
    fn default() -> Self {
        Redactor::Full
    }
}

impl FromStr for Redactor {
    type Err = &'static str;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        use Redactor::*;

        match s {
            "full" => Ok(Full),
            _ => Err("unknown redactor"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use regex::Regex;

    test_type_def![
        string_infallible {
            expr: |_| RedactFn {
                value: lit!("foo").boxed(),
                filters: vec![Filter::Pattern],
                patterns: None,
                redactor: Redactor::Full,
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        non_string_fallible {
            expr: |_| RedactFn {
                value: lit!(27).boxed(),
                filters: vec![Filter::Pattern],
                patterns: None,
                redactor: Redactor::Full,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        valid_pattern_infallible {
            expr: |_| RedactFn {
                value: lit!("1111222233334444").boxed(),
                filters: vec![Filter::Pattern],
                patterns: Some(vec![Literal::from(Regex::new(r"/[0-9]{16}/").unwrap()).into()]),
                redactor: Redactor::Full,
            },
            def: TypeDef {
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }

        invalid_pattern_fallible {
            expr: |_| RedactFn {
                value: lit!("1111222233334444").boxed(),
                filters: vec![Filter::Pattern],
                patterns: Some(vec![lit!("i am a teapot").into()]),
                redactor: Redactor::Full,
            },
            def: TypeDef {
                fallible: true,
                kind: value::Kind::Bytes,
                ..Default::default()
            },
        }
    ];
}
*/
