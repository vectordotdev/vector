use crate::{parser, Error, Expr, Result};
use pest::Parser;
use std::str::FromStr;

/// The program to execute.
///
/// This object is passed to [`Runtime::execute`](crate::Runtime::execute).
///
/// You can create a program using [`Program::from_str`]. The provided string
/// will be parsed. If parsing fails, an [`Error`] is returned.
#[derive(Debug)]
pub struct Program {
    pub(crate) expressions: Vec<Expr>,
}

impl FromStr for Program {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let pairs = parser::Parser::parse(parser::Rule::program, s)
            .map_err(|s| Error::Parser(s.to_string()))?;

        let expressions = parser::pairs_to_expressions(pairs)?;

        Ok(Self { expressions })
    }
}
