#![deny(
    warnings,
    clippy::all,
    clippy::pedantic,
    unreachable_pub,
    unused_allocation,
    unused_extern_crates,
    unused_assignments,
    unused_comparisons
)]
#![allow(
    clippy::match_on_vec_items, // allowed in initial deny commit
    clippy::missing_errors_doc, // allowed in initial deny commit
    clippy::semicolon_if_nothing_returned, // allowed in initial deny commit
    clippy::too_many_lines, // allowed in initial deny commit
)]

use std::borrow::ToOwned;

use lalrpop_util::lalrpop_mod;
lalrpop_mod!(
    #[allow(
        warnings,
        clippy::all,
        clippy::pedantic,
        unreachable_pub,
        unused_allocation,
        unused_extern_crates,
        unused_assignments,
        unused_comparisons
    )]
    parser
);

#[cfg(feature = "fuzz")]
mod arbitrary;
#[cfg(feature = "fuzz")]
mod arbitrary_depth;
pub mod ast;
mod lex;
mod template_string;

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
                .map_token(|t| t.map(ToOwned::to_owned))
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
                .map_token(|t| t.map(ToOwned::to_owned))
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
                .map_token(|t| t.map(ToOwned::to_owned))
                .map_error(|err| err.to_string()),
            dropped_tokens: vec![],
        })
}
