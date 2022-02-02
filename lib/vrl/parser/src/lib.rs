use lalrpop_util::lalrpop_mod;
lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    parser
);

mod arbitrary_depth;
pub mod ast;
mod lex;

pub use ast::{Literal, Program};
pub use diagnostic::Span;
pub use lex::{Error, Token};
use lookup::LookupBuf;

pub fn parse(input: impl AsRef<str>) -> Result<Program, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::ProgramParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}

pub fn parse_path(input: impl AsRef<str>) -> Result<LookupBuf, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::QueryParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
        .and_then(|query| match query.target.into_inner() {
            ast::QueryTarget::External => Ok(query.path.into_inner()),
            _ => Err(Error::UnexpectedParseError(
                "unexpected query target".to_owned(),
            )),
        })
}

pub fn parse_literal(input: impl AsRef<str>) -> Result<Literal, Error> {
    let lexer = lex::Lexer::new(input.as_ref());

    parser::LiteralParser::new()
        .parse(input.as_ref(), lexer)
        .map_err(|source| Error::ParseError {
            span: Span::new(0, input.as_ref().len()),
            source: source
                .map_token(|t| t.map(|s| s.to_owned()))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}

pub mod test {
    pub use super::parser::TestParser as Parser;
}
