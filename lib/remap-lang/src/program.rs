use crate::{parser::Parser, value, Error as E, Expr, Expression, Function, RemapError, TypeDef};
use std::fmt;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error(transparent)]
    ResolvesTo(#[from] ResolvesToError),

    #[error("expected to be infallible, but is not")]
    Fallible,
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub struct ResolvesToError(TypeDef, TypeDef);

impl fmt::Display for ResolvesToError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let want = &self.0;
        let got = &self.1;

        let mut want_str = "".to_owned();
        let mut got_str = "".to_owned();

        if want.is_fallible() != got.is_fallible() {
            if want.is_fallible() {
                want_str.push_str("an error, or ");
            }

            if got.is_fallible() {
                got_str.push_str("an error, or ");
            }
        }

        want_str.push_str(&format!("{} value", want.kind));
        got_str.push_str(&format!("{} value", got.kind));

        let want_kinds: Vec<_> = want.kind.into_iter().collect();
        let got_kinds: Vec<_> = got.kind.into_iter().collect();

        if !want.kind.is_all() && want_kinds.len() > 1 {
            want_str.push('s');
        }

        if !got.kind.is_all() && got_kinds.len() > 1 {
            got_str.push('s');
        }

        write!(
            f,
            "expected to resolve to {}, but instead resolves to {}",
            want_str, got_str
        )
    }
}

/// The constraint applied to the result of a program.
pub struct TypeConstraint {
    /// The type definition constraint for the program.
    pub type_def: TypeDef,

    /// If set to `true`, then a program that can return "any" value is
    /// considered to be valid.
    ///
    /// Note that the configured `type_def.kind` value still holds when a
    /// program returns anything other than any.
    ///
    /// Meaning, if a program returns a boolean or a string, and the constraint
    /// is set to return a float, and `allow_any` is set to `true`, the
    /// constraint will fail.
    ///
    /// However, for the same configuration, if the program returns "any" value,
    /// the constraint holds, unless `allow_any` is set to `false`.
    pub allow_any: bool,
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
        constraint: Option<TypeConstraint>,
    ) -> Result<Self, RemapError> {
        let mut parser = Parser::new(function_definitions);
        let expressions = parser.program_from_str(source)?;

        // optional type constraint checking
        if let Some(constraint) = constraint {
            let mut type_defs = expressions
                .iter()
                .map(|e| e.type_def(&parser.compiler_state))
                .collect::<Vec<_>>();

            let program_def = type_defs.pop().unwrap_or(TypeDef {
                fallible: true,
                kind: value::Kind::Null,
                ..Default::default()
            });

            if !constraint.type_def.contains(&program_def)
                && (!program_def.kind.is_all() || !constraint.allow_any)
            {
                return Err(RemapError::from(E::from(Error::ResolvesTo(
                    ResolvesToError(constraint.type_def, program_def),
                ))));
            }

            if !constraint.type_def.is_fallible() && type_defs.iter().any(TypeDef::is_fallible) {
                return Err(RemapError::from(E::from(Error::Fallible)));
            }
        }

        Ok(Self { expressions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;
    use std::error::Error;

    #[test]
    fn program_test() {
        use value::Kind;

        let cases = vec![
            (
                ".foo",
                None,
                Ok(()),
            ),
            // "any" value is allowed
            (
                ".foo",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: true,
                        kind: Kind::Boolean,
                        ..Default::default()
                    },
                    allow_any: true,
                }),
                Ok(()),
            ),
            // "any" value is allowed, but resolves to non-allowed kind
            (
                "42",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: false,
                        kind: Kind::Boolean,
                        ..Default::default()
                    },
                    allow_any: true,
                }),
                Err("expected to resolve to boolean value, but instead resolves to integer value"),
            ),
            // "any" value is disallowed, and resolves to any
            (
                ".foo",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: true,
                        kind: Kind::Boolean,
                        ..Default::default()
                    },
                    allow_any: false,
                }),
                Err("expected to resolve to boolean value, but instead resolves to any value"),
            ),
            // The final expression is infallible, but the first one isn't, so
            // this isn't allowed.
            (
                ".foo\ntrue",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: false,
                        ..Default::default()
                    },
                    allow_any: false,
                }),
                Err("expected to be infallible, but is not"),
            ),
            (
                ".foo",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: false,
                        ..Default::default()
                    },
                    allow_any: false,
                }),
                Err("expected to resolve to any value, but instead resolves to an error, or any value"),
            ),
            (
                ".foo",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: false,
                        kind: Kind::Bytes,
                        ..Default::default()
                    },
                    allow_any: false,
                }),
                Err("expected to resolve to string value, but instead resolves to an error, or any value"),
            ),
            (
                "false || 2",
                Some(TypeConstraint {
                    type_def: TypeDef {
                        fallible: false,
                        kind: Kind::Bytes | Kind::Float,
                        ..Default::default()
                    },
                    allow_any: false,
                }),
                Err("expected to resolve to string or float values, but instead resolves to integer or boolean values"),
            ),
        ];

        for (source, constraint, expect) in cases {
            let program = Program::new(source, &[], constraint)
                .map(|_| ())
                .map_err(|e| {
                    e.source()
                        .and_then(|e| e.source().map(|e| e.to_string()))
                        .unwrap()
                });

            assert_eq!(program, expect.map_err(ToOwned::to_owned));
        }
    }
}
