use crate::{
    expression, function,
    parser::{self, Rule},
    path, program, value,
};
use std::error::Error as StdError;
use std::fmt;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
    #[error("parser error")]
    Parser(#[from] parser::Error),

    #[error("program error")]
    Program(#[from] program::Error),

    #[error("unexpected token sequence")]
    Rule(#[from] Rule),

    #[error(transparent)]
    Expression(#[from] expression::Error),

    #[error("function error")]
    Function(#[from] function::Error),

    #[error("value error")]
    Value(#[from] value::Error),

    #[error("function call error: {0}")]
    Call(String),

    #[error("assertion failed: {0}")]
    Assert(String),

    #[error("path error")]
    Path(#[from] path::Error),

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

impl StdError for Rule {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
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
            array,
            assignment,
            block,
            boolean,
            boolean_expr,
            call,
            char,
            comparison,
            EOE,
            EOI,
            equality,
            expression,
            expressions,
            float,
            group,
            ident,
            if_condition,
            if_statement,
            integer,
            kv_pair,
            map,
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
            path_segments,
            primary,
            program,
            regex,
            regex_char,
            regex_flags,
            regex_inner,
            rule_ident,
            rule_path,
            rule_string_inner,
            string,
            string_inner,
            target,
            value,
            variable,
            WHITESPACE,
        ]
    }
}

#[derive(Debug, PartialEq)]
pub struct RemapError(Error);

impl StdError for RemapError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.0)
    }
}

impl fmt::Display for RemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("remap error")?;

        let mut error: &(dyn StdError + 'static) = self;
        while let Some(current) = error.source() {
            error = current;
            write!(f, ": {}", error)?;
        }

        Ok(())
    }
}

impl From<Error> for RemapError {
    fn from(error: Error) -> Self {
        RemapError(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_error() {
        let error1 = expression::function::Error::Required("arg1".to_owned(), 0);
        let error2 = expression::Error::Function("foo_func".to_owned(), error1);
        let error3 = Error::Expression(error2);
        let error = RemapError(error3);

        assert_eq!(
            r#"remap error: error for function "foo_func": missing required argument "arg1" (position 0)"#.to_owned(),
            error.to_string(),
        );
    }
}
