use crate::{expression, function, parser::Rule, value};
use std::fmt;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("parser error: {0}")]
    Parser(String),

    #[error("invalid escape char: {0}")]
    EscapeChar(char),

    #[error("unexpected token sequence")]
    Rule(#[from] Rule),

    #[error(transparent)]
    Expression(#[from] expression::Error),

    #[error("function error")]
    Function(#[from] function::Error),

    #[error("regex error")]
    Regex(#[from] regex::Error),

    #[error("value error")]
    Value(#[from] value::Error),

    #[error("function call error: {0}")]
    Call(String),

    #[error("unknown error")]
    Unknown,
}

impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Call(s)
    }
}

impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Error::Call(s.to_owned())
    }
}

impl std::error::Error for Rule {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        macro_rules! rules_str {
            ($($rule:tt),+ $(,)?) => (
                match self {
                    $(Rule::$rule => f.write_str(stringify!($rule))),+
                }
            );
        }

        rules_str![
            addition,
            argument,
            arguments,
            assignment,
            block,
            boolean,
            boolean_expr,
            call,
            char,
            comparison,
            EOI,
            equality,
            expression,
            float,
            group,
            ident,
            if_statement,
            integer,
            multiplication,
            not,
            null,
            operator_addition,
            operator_boolean_expr,
            operator_comparison,
            operator_equality,
            operator_multiplication,
            operator_not,
            path,
            path_coalesce,
            path_field,
            path_index,
            path_index_inner,
            path_segment,
            primary,
            program,
            regex,
            regex_char,
            regex_flags,
            regex_inner,
            string,
            string_inner,
            target,
            value,
            WHITESPACE,
        ]
    }
}
