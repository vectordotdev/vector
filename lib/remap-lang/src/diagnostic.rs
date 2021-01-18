use crate::value::Kind;
use codespan_reporting::diagnostic;
use std::fmt;
use std::ops::{Deref, DerefMut, Range, RangeInclusive};

/// A result type in which the `Ok` variant contains `T` and a list of zero or
/// more non-error diagnostics. The `Err` variant contains a list of one or more
/// diagnostics (errors and warnings).
pub type Result<T> = std::result::Result<(T, DiagnosticList), DiagnosticList>;

/// A formatter to display diagnostics tied to a given source.
pub struct Formatter<'a> {
    source: &'a str,
    diagnostics: DiagnosticList,
    color: bool,
}

impl<'a> Formatter<'a> {
    pub fn new(source: &'a str, diagnostics: impl Into<DiagnosticList>) -> Self {
        Self {
            source,
            diagnostics: diagnostics.into(),
            color: false,
        }
    }

    pub fn colored(mut self) -> Self {
        self.color = true;
        self
    }

    pub fn enable_colors(&mut self, color: bool) {
        self.color = color
    }
}

impl<'a> fmt::Display for Formatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use codespan_reporting::files::SimpleFile;
        use codespan_reporting::term;
        use std::str::from_utf8;
        use termcolor::Buffer;

        let file = SimpleFile::new("", self.source);
        let config = term::Config::default();
        let mut buffer = if self.color {
            Buffer::ansi()
        } else {
            Buffer::no_color()
        };

        f.write_str("\n")?;

        for diagnostic in self.diagnostics.iter() {
            term::emit(&mut buffer, &config, &file, &diagnostic.to_owned().into())
                .map_err(|_| fmt::Error)?;
        }

        f.write_str(from_utf8(buffer.as_slice()).map_err(|_| fmt::Error)?)
    }
}

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

impl From<Vec<Diagnostic>> for DiagnosticList {
    fn from(diagnostics: Vec<Diagnostic>) -> Self {
        Self(diagnostics)
    }
}

impl From<Diagnostic> for DiagnosticList {
    fn from(diagnostic: Diagnostic) -> Self {
        Self(vec![diagnostic])
    }
}

// -----------------------------------------------------------------------------

/// A span pointing into the program source.
///
/// This exists because `Range` doesn't implement `Copy` and to make it easy to
/// convert other types into spans.
///
/// Similar to `Range`, the range is half-open, meaning `end` is exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Span { start, end }
    }
}

impl From<Span> for Range<usize> {
    fn from(span: Span) -> Self {
        span.start..span.end
    }
}

impl From<Range<usize>> for Span {
    fn from(range: Range<usize>) -> Self {
        Span {
            start: range.start,
            end: range.end,
        }
    }
}

impl From<RangeInclusive<usize>> for Span {
    fn from(range: RangeInclusive<usize>) -> Self {
        let (start, end) = range.into_inner();

        Span {
            start,
            end: end + 1,
        }
    }
}

impl From<&str> for Span {
    fn from(source: &str) -> Self {
        (0..source.bytes().len().saturating_sub(1)).into()
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Severity {
    Bug,
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Returns `true` if the severity is a [bug](Variant::Bug).
    #[inline]
    pub fn is_bug(self) -> bool {
        matches!(self, Severity::Bug)
    }

    /// Returns `true` if the severity is an [error](Variant::Error).
    #[inline]
    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }

    /// Returns `true` if the severity is a [warning](Variant::Warning).
    #[inline]
    pub fn is_warning(self) -> bool {
        matches!(self, Severity::Warning)
    }

    /// Returns `true` if the severity is a [note](Variant::Note).
    #[inline]
    pub fn is_note(self) -> bool {
        matches!(self, Severity::Note)
    }
}

impl Into<diagnostic::Severity> for Severity {
    fn into(self) -> diagnostic::Severity {
        use Severity::*;

        match self {
            Bug => diagnostic::Severity::Bug,
            Error => diagnostic::Severity::Error,
            Warning => diagnostic::Severity::Warning,
            Note => diagnostic::Severity::Note,
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Clone)]
pub enum Note {
    ExpectedKind(Kind),
    CoerceValue,
    InfallibleAssignment {
        ok: String,
        err: String,
    },
    SeeFuncDocs(&'static str),
    SeeErrDocs,

    #[doc(hidden)]
    SeeLangDocs,
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Note::*;

        match self {
            ExpectedKind(kind) => write!(f, "expected: {}", kind),
            CoerceValue => {
                f.write_str("hint: coerce the value using one of the coercion functions")
            }
            InfallibleAssignment { ok, err } => {
                write!(
                    f,
                    r#"hint: assign to "{}", without assigning to "{}""#,
                    ok, err
                )
            }
            SeeFuncDocs(func) => {
                write!(f,
                "see function documentation at: https://master.vector.dev/docs/reference/remap/#{}",
                func
            )
            }
            SeeErrDocs => f.write_str(
                "see error handling documentation at: https://vector.dev/docs/reference/vrl/",
            ),
            SeeLangDocs => {
                f.write_str("see language documentation at: https://vector.dev/docs/reference/vrl/")
            }
        }
    }
}

// -----------------------------------------------------------------------------

#[derive(Debug, PartialEq, Clone)]
pub struct Label {
    pub message: String,
    pub primary: bool,
    pub span: Span,
}

impl Label {
    pub fn primary(message: impl ToString, span: impl Into<Span>) -> Self {
        Self {
            message: message.to_string(),
            primary: true,
            span: span.into(),
        }
    }

    pub fn context(message: impl ToString, span: impl Into<Span>) -> Self {
        Self {
            message: message.to_string(),
            primary: false,
            span: span.into(),
        }
    }
}

impl Into<diagnostic::Label<()>> for Label {
    fn into(self) -> diagnostic::Label<()> {
        let style = match self.primary {
            true => diagnostic::LabelStyle::Primary,
            false => diagnostic::LabelStyle::Secondary,
        };

        diagnostic::Label {
            style,
            file_id: (),
            range: self.span.into(),
            message: self.message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_from_range() {
        let range = 10..20;
        let span = Span::from(range);

        assert_eq!(span.start, 10);
        assert_eq!(span.end, 20);
    }

    #[test]
    fn test_span_from_range_inclusive() {
        let range = 10..=20;
        let span = Span::from(range);

        assert_eq!(span.start, 10);
        assert_eq!(span.end, 21);
    }
}
