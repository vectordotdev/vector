use lalrpop_util::lalrpop_mod;
lalrpop_mod!(
    #[allow(clippy::all)]
    #[allow(unused)]
    parser
);

pub mod ast;
mod lex;

pub use ast::{Literal, Program};
pub use lex::{Error, Token};
use lookup::LookupBuf;
pub use vrl_core::diagnostic::Span;

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

#[cfg(test)]
mod tests {
    use super::*;
    use vrl_core::value;

    #[test]
    fn test_object() {
        let path = parse_path(".foo.bar.baz").unwrap();
        let value = value!(12);

        let object = value!({ "foo": { "bar": { "baz": 12 } } });

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_root() {
        let path = parse_path(".").unwrap();
        let value = value!(12);

        let object = value!(12);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_array() {
        let path = parse_path(".[2]").unwrap();
        let value = value!(12);

        let object = value!([null, null, 12]);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_complex() {
        let path = parse_path(".[2].foo.(bar | baz )[1]").unwrap();
        let value = value!({ "bar": [12] });

        let object = value!([null, null, { "foo": { "baz": [null, { "bar": [12] }] } } ]);

        assert_eq!(value.at_path(&path), object);
    }
}
