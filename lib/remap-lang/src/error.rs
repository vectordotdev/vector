use crate::{
    diagnostic::{Diagnostic, DiagnosticList, Note},
    expression, function,
    parser::{ParsedExpression, Rule},
    path, program, value, Expr,
};
use std::error::Error as StdError;
use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError<'a> {
    source: &'a str,
    diagnostic: Diagnostic,
}

impl RuntimeError<'_> {
    pub fn diagnostic(&self) -> &Diagnostic {
        &self.diagnostic
    }
}

impl fmt::Display for RuntimeError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut diagnostics = DiagnosticList::default();
        diagnostics.push(self.diagnostic.clone());

        fmt_diagnostic(f, self.source, diagnostics)
    }
}

impl StdError for RuntimeError<'_> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        None
    }
}

impl<'a> From<(&'a str, &'a ParsedExpression, Error)> for RuntimeError<'a> {
    fn from((source, expr, error): (&'a str, &'a ParsedExpression, Error)) -> Self {
        let span = expr.span();
        let mut diagnostic = Diagnostic::error("program aborted");

        diagnostic = match error {
            Error::Call(err) => {
                diagnostic = diagnostic
                    .with_primary("function call error", span)
                    .with_primary(err, span);

                if let Expr::Function(func) = expr.expression() {
                    diagnostic = diagnostic.with_note(Note::SeeFuncDocs(func.ident()))
                };

                diagnostic
            }
            Error::Function(err) => match err {
                _ => diagnostic.with_primary("todo", expr.span()),
            },
            _ => diagnostic.with_primary(error.to_string(), expr.span()),
        };

        Self { source, diagnostic }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgramError<'a> {
    source: &'a str,
    diagnostics: DiagnosticList,
}

impl fmt::Display for ProgramError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt_diagnostic(f, self.source, self.diagnostics.clone())
    }
}

impl StdError for ProgramError<'_> {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        None
    }
}

impl<'a> From<(&'a str, DiagnosticList)> for ProgramError<'a> {
    fn from((source, diagnostics): (&'a str, DiagnosticList)) -> Self {
        Self {
            source,
            diagnostics,
        }
    }
}

fn fmt_diagnostic(
    f: &mut fmt::Formatter<'_>,
    source: &str,
    diagnostics: DiagnosticList,
) -> fmt::Result {
    use codespan_reporting::files::SimpleFile;
    use codespan_reporting::term;
    use std::str::from_utf8;
    use termcolor::Buffer;

    let file = SimpleFile::new("", source);
    let config = term::Config::default();

    let mut buffer = if f.alternate() {
        Buffer::ansi()
    } else {
        Buffer::no_color()
    };

    for diagnostic in diagnostics {
        term::emit(&mut buffer, &config, &file, &diagnostic.into()).map_err(|_| fmt::Error)?;
    }

    f.write_str(from_utf8(buffer.as_slice()).map_err(|_| fmt::Error)?)
}

#[derive(thiserror::Error, Clone, Debug, PartialEq)]
pub enum Error {
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
