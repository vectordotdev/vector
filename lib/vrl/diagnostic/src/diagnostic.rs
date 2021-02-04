use crate::{DiagnosticError, Label, Note, Severity, Span};
use codespan_reporting::diagnostic;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    severity: Severity,
    message: String,
    labels: Vec<Label>,
    notes: Vec<Note>,
}

impl Diagnostic {
    pub fn error(message: impl ToString) -> Self {
        Self::new(Severity::Error, message, vec![], vec![])
    }

    pub fn bug(message: impl ToString) -> Self {
        Self::new(Severity::Bug, message, vec![], vec![])
    }

    pub fn new(
        severity: Severity,
        message: impl ToString,
        labels: Vec<Label>,
        notes: Vec<Note>,
    ) -> Self {
        Self {
            severity,
            message: message.to_string(),
            labels,
            notes,
        }
    }

    pub fn with_primary(self, message: impl ToString, span: impl Into<Span>) -> Self {
        self.with_label(Label::primary(message, span.into()))
    }

    pub fn with_context(self, message: impl ToString, span: impl Into<Span>) -> Self {
        self.with_label(Label::context(message, span.into()))
    }

    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    pub fn with_note(mut self, note: Note) -> Self {
        self.notes.push(note);
        self
    }

    pub fn severity(&self) -> Severity {
        self.severity
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// Returns `true` if the diagnostic represents either an
    /// [error](Variant::Error) or [bug](Variant::Bug).
    #[inline]
    pub fn is_problem(&self) -> bool {
        self.severity.is_error() || self.severity.is_bug()
    }

    /// Returns `true` if the diagnostic represents a [bug](Variant::Bug).
    #[inline]
    pub fn is_bug(&self) -> bool {
        self.severity.is_bug()
    }

    /// Returns `true` if the diagnostic represents an [error](Variant::Error).
    #[inline]
    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    /// Returns `true` if the diagnostic represents a
    /// [warning](Variant::Warning).
    #[inline]
    pub fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }

    /// Returns `true` if the diagnostic represents a [note](Variant::Note).
    #[inline]
    pub fn is_note(&self) -> bool {
        self.severity.is_note()
    }
}

impl From<Box<dyn DiagnosticError>> for Diagnostic {
    fn from(error: Box<dyn DiagnosticError>) -> Self {
        Self {
            severity: Severity::Error,
            message: error.message(),
            labels: error.labels(),
            notes: error.notes(),
        }
    }
}

impl Into<diagnostic::Diagnostic<()>> for Diagnostic {
    fn into(self) -> diagnostic::Diagnostic<()> {
        let mut notes = self.notes.to_vec();
        notes.push(Note::SeeLangDocs);

        diagnostic::Diagnostic {
            severity: self.severity.into(),
            code: None,
            message: self.message.to_string(),
            labels: self.labels.to_vec().into_iter().map(Into::into).collect(),
            notes: notes.iter().map(ToString::to_string).collect(),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiagnosticList(Vec<Diagnostic>);

impl DiagnosticList {
    /// Turns the diagnostic list into a result type, the `Ok` variant is
    /// returned if none of the diagnostics are errors or bugs. Otherwise the
    /// `Err` variant is returned.
    pub fn into_result(self) -> std::result::Result<DiagnosticList, DiagnosticList> {
        if self.is_err() {
            return Err(self);
        }

        Ok(self)
    }

    /// Returns `true` if there are any errors or bugs in the parsed source.
    pub fn is_err(&self) -> bool {
        self.0.iter().any(|d| d.is_problem())
    }

    /// Returns the list of bug-level diagnostics.
    pub fn bugs(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_bug()).collect()
    }

    /// Returns the list of error-level diagnostics.
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_error()).collect()
    }

    /// Returns the list of warning-level diagnostics.
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_warning()).collect()
    }

    /// Returns the list of note-level diagnostics.
    pub fn notes(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_note()).collect()
    }

    /// Returns `true` if there are any bug diagnostics.
    pub fn has_bugs(&self) -> bool {
        self.0.iter().any(|d| d.is_bug())
    }

    /// Returns `true` if there are any error diagnostics.
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(|d| d.is_error())
    }

    /// Returns `true` if there are any warning diagnostics.
    pub fn has_warnings(&self) -> bool {
        self.0.iter().any(|d| d.is_warning())
    }

    /// Returns `true` if there are any note diagnostics.
    pub fn has_notes(&self) -> bool {
        self.0.iter().any(|d| d.is_note())
    }
}

impl Deref for DiagnosticList {
    type Target = Vec<Diagnostic>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for DiagnosticList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoIterator for DiagnosticList {
    type Item = Diagnostic;
    type IntoIter = std::vec::IntoIter<Diagnostic>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T: Into<Diagnostic>> From<Vec<T>> for DiagnosticList {
    fn from(diagnostics: Vec<T>) -> Self {
        Self(diagnostics.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<Diagnostic>> From<T> for DiagnosticList {
    fn from(diagnostic: T) -> Self {
        Self(vec![diagnostic.into()])
    }
}
