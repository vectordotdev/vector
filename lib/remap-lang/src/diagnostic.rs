use crate::value::Kind;
use codespan_reporting::diagnostic;
use std::fmt;
use std::ops::Range;

#[derive(Debug, Clone, PartialEq)]
pub struct Diagnostic {
    pub(crate) severity: Severity,
    pub(crate) message: Message,
    pub(crate) labels: Vec<Label>,
    pub(crate) notes: Vec<Note>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Severity {
    Bug,
    Error,
    Warning,
    Note,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Message {
    Parse,
    Fallible,
    ReturnValue,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LabelMessage {
    ResolvesToKind(Kind),
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Note {
    ExpectedKind(Kind),
    CoerceValue,

    #[doc(hidden)]
    SeeLangDocs,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Label {
    pub message: LabelMessage,
    pub primary: bool,
    pub range: (usize, usize),
}

// -----------------------------------------------------------------------------

impl Label {
    pub fn primary(message: LabelMessage, range: Range<usize>) -> Self {
        Self {
            message,
            primary: true,
            range: (range.start, range.end),
        }
    }

    pub fn context(message: LabelMessage, range: Range<usize>) -> Self {
        Self {
            message,
            primary: false,
            range: (range.start, range.end),
        }
    }
}

// -----------------------------------------------------------------------------

impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Message::*;

        match self {
            Parse => f.write_str("parse error"),
            Fallible => f.write_str("uncaught error"),
            ReturnValue => f.write_str("unexpected return value"),
        }
    }
}

impl fmt::Display for LabelMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use LabelMessage::*;

        match self {
            ResolvesToKind(kind) => write!(f, "resolves to {}", kind),
        }
    }
}

impl fmt::Display for Note {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Note::*;

        match self {
            ExpectedKind(kind) => write!(f, "resolves to {}", kind),
            CoerceValue => f.write_str("coerce the value using one of the coercion functions"),
            SeeLangDocs => f.write_str("see language documentation at: https://vector.dev"),
        }
    }
}

// -----------------------------------------------------------------------------

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

impl Into<diagnostic::Label<()>> for Label {
    fn into(self) -> diagnostic::Label<()> {
        let style = match self.primary {
            true => diagnostic::LabelStyle::Primary,
            false => diagnostic::LabelStyle::Secondary,
        };

        diagnostic::Label {
            style,
            file_id: (),
            range: self.range.0..self.range.1,
            message: self.message.to_string(),
        }
    }
}
