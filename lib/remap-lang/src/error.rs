use crate::expression::{self, assignment, constant, function, if_statement, not, path, variable};
use crate::parser::Rule;
use crate::value;
use std::fmt;

#[derive(thiserror::Error, Debug, PartialEq)]
pub enum Error {
    #[error("parser error: {0}")]
    Parser(String),

    #[error("invalid escape char: {0}")]
    EscapeChar(char),

    #[error("unexpected token sequence")]
    Rule(#[from] Rule),

    #[error("runtime error")]
    Runtime,

    #[error("expression error")]
    Expression(#[from] expression::Error),

    #[error(r#"error for function "{0}""#)]
    Function(String, #[source] function::Error),

    #[error("regex error")]
    Regex(#[from] regex::Error),

    #[error("assignment error")]
    Assignment(#[from] assignment::Error),

    #[error("value error")]
    Value(#[from] value::Error),

    #[error("path error")]
    Path(#[from] path::Error),

    #[error("not operation error")]
    Not(#[from] not::Error),

    #[error("if-statement error")]
    IfStatement(#[from] if_statement::Error),

    #[error("constant error")]
    Constant(#[from] constant::Error),

    #[error("variable error")]
    Variable(#[from] variable::Error),

    #[error("program manually aborted{}", match .0 {
        Some(s) => format!(": {}", s),
        None => "".to_owned(),
    })]
    Abort(Option<String>),

    #[error("unknown error")]
    Unknown,
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
            abort,
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
            constant,
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
            statement,
            string,
            string_inner,
            target,
            value,
            variable,
            WHITESPACE,
        ]
    }
}
