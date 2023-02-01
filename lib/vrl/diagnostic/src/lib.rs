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
    clippy::match_bool, // allowed in initial deny commit
    clippy::missing_errors_doc, // allowed in initial deny commit
    clippy::module_name_repetitions, // allowed in initial deny commit
    clippy::semicolon_if_nothing_returned,  // allowed in initial deny commit
    clippy::needless_pass_by_value,  // allowed in initial deny commit
)]

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
pub trait DiagnosticMessage: std::error::Error {
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

    /// The severity of the message.
    ///
    /// Defaults to `error`.
    fn severity(&self) -> Severity {
        Severity::Error
    }
}

pub struct Urls;

impl Urls {
    fn vrl_root_url() -> String {
        VRL_DOCS_ROOT_URL.into()
    }

    #[must_use]
    pub fn func_docs(ident: &str) -> String {
        format!("{VRL_FUNCS_ROOT_URL}/{ident}")
    }

    fn error_handling_url() -> String {
        format!("{VRL_ERROR_DOCS_ROOT_URL}/#handling")
    }

    fn error_code_url(code: usize) -> String {
        format!("{VRL_ERROR_DOCS_ROOT_URL}/{code}")
    }

    #[must_use]
    pub fn expression_docs_url(expr: &str) -> String {
        format!("{VRL_DOCS_ROOT_URL}/expressions/{expr}")
    }

    fn example_docs() -> String {
        format!("{VRL_DOCS_ROOT_URL}/examples")
    }
}
