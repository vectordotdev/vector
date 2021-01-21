use crate::{expression, function, parser::Rule, value};
use std::error::Error as StdError;
use std::fmt;

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
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
            ($($rule:tt$(: $name:literal)?),+ $(,)?) => (
                match self {
                    $(Rule::$rule => {
                        #[allow(unused_variables)]
                        let string = stringify!($rule);

                        // Comment out the next two lines when debugging to see
                        // the original rule names in error messages.
                        $(let string = $name;)?
                        let string = string.replace('_', " ");

                        f.write_str(&string)
                    }),+
                }
            );
        }

        rules_str![
            addition,
            argument,
            arguments,
            array,
            assignment,
            bang: "",
            block,
            boolean,
            boolean_expr,
            call,
            char,
            comparison,
            EOE: "",
            EOI: "",
            empty_line,
            equality,
            expression,
            expressions,
            field,
            float,
            group,
            ident: "",
            if_condition,
            if_statement: "if-statement",
            integer,
            kv_pair,
            map,
            multiplication,
            not: "query",
            null,
            operator_addition: "",
            operator_boolean_expr: "",
            operator_comparison: "",
            operator_equality: "",
            operator_multiplication: "operator",
            operator_not: "function call, value, variable, path, group, !",
            path,
            path_coalesce: "coalesced path",
            path_field,
            path_index,
            path_index_inner,
            path_segment,
            path_segments,
            primary: "value, variable, path, group",
            program,
            regex,
            regex_char,
            regex_flags,
            regex_inner,
            reserved_keyword,
            rule_ident,
            rule_path,
            rule_string_inner,
            string,
            string_inner,
            target,
            target_infallible,
            target_regular,
            value,
            variable,
            COMMENT,
            WHITESPACE,
        ]
    }
}
