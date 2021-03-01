mod diagnostic;
mod formatter;
mod label;
mod note;
mod severity;
mod span;

pub use diagnostic::{Diagnostic, DiagnosticList};
pub use formatter::Formatter;
pub use label::Label;
pub use note::Note;
pub use severity::Severity;
pub use span::{span, Span};

const VRL_DOCS_ROOT_URL: &str = "https://vrl.dev";
const VRL_ERROR_DOCS_ROOT_URL: &str = "https://errors.vrl.dev";
const VRL_FUNCS_ROOT_URL: &str = "https://functions.vrl.dev";

/// A trait that can be implemented by error types to provide diagnostic
/// information about the given error.
pub trait DiagnosticError: std::error::Error {
    fn code(&self) -> usize;

    /// The subject message of the error.
    ///
    /// Defaults to the error message itself.
    fn message(&self) -> String {
        self.to_string()
    }

    /// One or more labels to provide more context for a given error.
    ///
    /// Defaults to no labels.
    fn labels(&self) -> Vec<Label> {
        vec![]
    }

    /// One or more notes shown at the bottom of the diagnostic message.
    ///
    /// Defaults to no notes.
    fn notes(&self) -> Vec<Note> {
        vec![]
    }
}

pub struct Urls;

impl Urls {
    fn vrl_root_url() -> String {
        VRL_DOCS_ROOT_URL.into()
    }

    pub fn func_docs(ident: &str) -> String {
        format!("{}/{}", VRL_FUNCS_ROOT_URL, ident)
    }

    fn error_handling_url() -> String {
        format!("{}/#handling", VRL_ERROR_DOCS_ROOT_URL)
    }

    fn error_code_url(code: &usize) -> String {
        format!("{}/{}", VRL_ERROR_DOCS_ROOT_URL, code)
    }

    pub fn expression_docs_url(expr: &str) -> String {
        format!("{}/expressions/{}", VRL_DOCS_ROOT_URL, expr)
    }
}
