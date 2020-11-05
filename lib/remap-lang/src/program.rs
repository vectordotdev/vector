use crate::{parser, CompilerState, Error, Expr, Function, RemapError};
use pest::Parser;

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
    ) -> Result<Self, RemapError> {
        let pairs = parser::Parser::parse(parser::Rule::program, source)
            .map_err(|s| Error::Parser(s.to_string()))
            .map_err(RemapError)?;

        let compiler_state = CompilerState::default();

        let mut parser = parser::Parser {
            function_definitions,
            compiler_state,
        };

        let expressions = parser.pairs_to_expressions(pairs).map_err(RemapError)?;

        Ok(Self { expressions })
    }
}
