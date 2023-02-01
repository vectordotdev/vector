use std::ops::{Deref, DerefMut};

use codespan_reporting::diagnostic;

use crate::{DiagnosticMessage, Label, Note, Severity, Span};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: usize,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<Note>,
}

impl Diagnostic {
    pub fn error(code: usize, message: impl ToString) -> Self {
        Self::new(Severity::Error, code, message, vec![], vec![])
    }

    pub fn bug(code: usize, message: impl ToString) -> Self {
        Self::new(Severity::Bug, code, message, vec![], vec![])
    }

    pub fn new(
        severity: Severity,
        code: usize,
        message: impl ToString,
        labels: Vec<Label>,
        notes: Vec<Note>,
    ) -> Self {
        Self {
            severity,
            code,
            message: message.to_string(),
            labels,
            notes,
        }
    }

    #[must_use]
    pub fn with_primary(self, message: impl ToString, span: impl Into<Span>) -> Self {
        self.with_label(Label::primary(message, span.into()))
    }

    #[must_use]
    pub fn with_context(self, message: impl ToString, span: impl Into<Span>) -> Self {
        self.with_label(Label::context(message, span.into()))
    }

    #[must_use]
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    #[must_use]
    pub fn with_note(mut self, note: Note) -> Self {
        self.notes.push(note);
        self
    }

    #[must_use]
    pub fn severity(&self) -> Severity {
        self.severity
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn notes(&self) -> &[Note] {
        &self.notes
    }

    #[must_use]
    pub fn labels(&self) -> &[Label] {
        &self.labels
    }

    /// Returns `true` if the diagnostic represents either an
    /// [error](Severity::Error) or [bug](Severity::Bug).
    #[inline]
    #[must_use]
    pub fn is_problem(&self) -> bool {
        self.severity.is_error() || self.severity.is_bug()
    }

    /// Returns `true` if the diagnostic represents a [bug](Severity::Bug).
    #[inline]
    #[must_use]
    pub fn is_bug(&self) -> bool {
        self.severity.is_bug()
    }

    /// Returns `true` if the diagnostic represents an [error](Severity::Error).
    #[inline]
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity.is_error()
    }

    /// Returns `true` if the diagnostic represents a
    /// [warning](Severity::Warning).
    #[inline]
    #[must_use]
    pub fn is_warning(&self) -> bool {
        self.severity.is_warning()
    }

    /// Returns `true` if the diagnostic represents a [note](Severity::Note).
    #[inline]
    #[must_use]
    pub fn is_note(&self) -> bool {
        self.severity.is_note()
    }
}

impl From<Box<dyn DiagnosticMessage>> for Diagnostic {
    fn from(message: Box<dyn DiagnosticMessage>) -> Self {
        Self {
            severity: message.severity(),
            code: message.code(),
            message: message.message(),
            labels: message.labels(),
            notes: message.notes(),
        }
    }
}

impl From<Diagnostic> for diagnostic::Diagnostic<()> {
    fn from(diag: Diagnostic) -> Self {
        let mut notes = diag.notes.clone();

        // not all codes have a page on the site yet
        if diag.code >= 100 && diag.code <= 110 {
            notes.push(Note::SeeCodeDocs(diag.code));
        }

        notes.push(Note::SeeLangDocs);
        notes.push(Note::SeeRepl);

        diagnostic::Diagnostic {
            severity: diag.severity.into(),
            code: Some(format!("E{:03}", diag.code)),
            message: diag.message.to_string(),
            labels: diag.labels.iter().cloned().map(Into::into).collect(),
            notes: notes.iter().map(ToString::to_string).collect(),
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    #[must_use]
    pub fn is_err(&self) -> bool {
        self.0.iter().any(Diagnostic::is_problem)
    }

    /// Returns the list of bug-level diagnostics.
    #[must_use]
    pub fn bugs(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_bug()).collect()
    }

    /// Returns the list of error-level diagnostics.
    #[must_use]
    pub fn errors(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_error()).collect()
    }

    /// Returns the list of warning-level diagnostics.
    #[must_use]
    pub fn warnings(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_warning()).collect()
    }

    /// Returns the list of note-level diagnostics.
    #[must_use]
    pub fn notes(&self) -> Vec<&Diagnostic> {
        self.0.iter().filter(|d| d.is_note()).collect()
    }

    /// Returns `true` if there are any bug diagnostics.
    #[must_use]
    pub fn has_bugs(&self) -> bool {
        self.0.iter().any(Diagnostic::is_bug)
    }

    /// Returns `true` if there are any error diagnostics.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        self.0.iter().any(Diagnostic::is_error)
    }

    /// Returns `true` if there are any warning diagnostics.
    #[must_use]
    pub fn has_warnings(&self) -> bool {
        self.0.iter().any(Diagnostic::is_warning)
    }

    /// Returns `true` if there are any note diagnostics.
    #[must_use]
    pub fn has_notes(&self) -> bool {
        self.0.iter().any(Diagnostic::is_note)
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
