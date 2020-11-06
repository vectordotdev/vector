use crate::{
    parser, CompilerState, Error as E, Expr, Expression, Function, RemapError, ResolveKind,
};
use pest::Parser;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("program is expected to resolve to {0}, but instead resolves to {1}")]
    ResolvesTo(ResolveKind, ResolveKind),
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
        must_resolve_to: ResolveKind,
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

        let will_resolve_to = expressions
            .last()
            .map(|e| e.resolves_to(&parser.compiler_state))
            .unwrap_or_else(|| ResolveKind::Maybe(Box::new(ResolveKind::Any)));

        if !must_resolve_to.contains(&will_resolve_to) {
            return Err(RemapError::from(E::from(Error::ResolvesTo(
                must_resolve_to,
                will_resolve_to,
            ))));
        }

        Ok(Self { expressions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ValueKind;

    #[test]
    fn program_test() {
        use ResolveKind::*;

        let cases = vec![
            (".foo", Any, Ok(())),
            (
                ".foo",
                Exact(ValueKind::String),
                Err("remap error: program error: program is expected to resolve to string, but instead resolves to any value".to_owned()),
            ),
            (
                "false || 2",
                OneOf(vec![ValueKind::String, ValueKind::Float]),
                Err("remap error: program error: program is expected to resolve to any of string, float, but instead resolves to any of integer, boolean".to_owned()),
            ),
        ];

        for (source, must_resolve_to, expect) in cases {
            let program = Program::new(source, &[], must_resolve_to)
                .map(|_| ())
                .map_err(|e| e.to_string());

            assert_eq!(program, expect);
        }
    }
}
