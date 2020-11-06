use crate::{parser, CompilerState, Error as E, Expr, Expression, Function, RemapError, TypeCheck};
use pest::Parser;
use std::fmt;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    ResolvesTo(TypeCheck, TypeCheck),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (want, got) = match self {
            Error::ResolvesTo(want, got) => (want, got),
        };

        let fallible_diff = want.is_fallible() != got.is_fallible();
        let optional_diff = want.is_optional() != got.is_optional();

        let mut want_str = "".to_owned();
        let mut got_str = "".to_owned();

        if fallible_diff {
            if want.is_fallible() {
                want_str.push_str("an error, or ");
            }

            if got.is_fallible() {
                got_str.push_str("an error, or ");
            }
        }

        want_str.push_str(&want.constraint.to_string());
        got_str.push_str(&got.constraint.to_string());

        if optional_diff {
            if want.is_optional() {
                want_str.push_str(" optional");
            }

            if got.is_optional() {
                got_str.push_str(" optional");
            }
        } else {
            want_str.push_str(" value");
            got_str.push_str(" value");
        }

        let want_kinds = want.constraint.value_kinds();
        let got_kinds = got.constraint.value_kinds();

        if !want.constraint.is_any() && want_kinds.len() > 1 {
            want_str.push('s');
        }

        if !got.constraint.is_any() && got_kinds.len() > 1 {
            got_str.push('s');
        }

        write!(
            f,
            "expected to resolve to {}, but instead resolves to {}",
            want_str, got_str
        )
    }
}

/// The program to execute.
///
/// This object is passed to [`Runtime::execute`](crate::Runtime::execute).
///
/// You can create a program using [`Program::from_str`]. The provided string
/// will be parsed. If parsing fails, an [`Error`] is returned.
#[derive(Debug, Clone)]
pub struct Program {
    pub(crate) expressions: Vec<Expr>,
}

impl Program {
    pub fn new(
        source: &str,
        function_definitions: &[Box<dyn Function>],
        expected_result: TypeCheck,
    ) -> Result<Self, RemapError> {
        let pairs = parser::Parser::parse(parser::Rule::program, source)
            .map_err(|s| E::Parser(s.to_string()))
            .map_err(RemapError)?;

        let compiler_state = CompilerState::default();

        let mut parser = parser::Parser {
            function_definitions,
            compiler_state,
        };

        let expressions = parser.pairs_to_expressions(pairs).map_err(RemapError)?;

        let computed_result = expressions
            .last()
            .map(|e| e.type_check(&parser.compiler_state))
            .unwrap_or(TypeCheck {
                optional: true,
                fallible: true,
                ..Default::default()
            });

        if !expected_result.contains(&computed_result) {
            return Err(RemapError::from(E::from(Error::ResolvesTo(
                expected_result,
                computed_result,
            ))));
        }

        Ok(Self { expressions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValueConstraint, ValueKind};
    use std::error::Error;

    #[test]
    fn program_test() {
        use ValueConstraint::*;
        use ValueKind::*;

        let cases = vec![
            (".foo", TypeCheck { fallible: true, ..Default::default()}, Ok(())),
            (
                ".foo",
                TypeCheck::default(),
                Err("expected to resolve to any value, but instead resolves to an error, or any value".to_owned()),
            ),
            (
                ".foo",
                TypeCheck {
                    fallible: false,
                    optional: false,
                    constraint: Exact(String),
                },
                Err("expected to resolve to string value, but instead resolves to an error, or any value".to_owned()),
            ),
            (
                "false || 2",

                TypeCheck {
                    fallible: false,
                    optional: false,
                    constraint: OneOf(vec![String, Float]),
                },
                Err("expected to resolve to string or float values, but instead resolves to an error, or integer or boolean values".to_owned()),
            ),
        ];

        for (source, expected_result, expect) in cases {
            let program = Program::new(source, &[], expected_result)
                .map(|_| ())
                .map_err(|e| {
                    e.source()
                        .and_then(|e| e.source().map(|e| e.to_string()))
                        .unwrap()
                });

            assert_eq!(program, expect);
        }
    }
}
