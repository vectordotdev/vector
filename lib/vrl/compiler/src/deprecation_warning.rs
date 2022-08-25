use crate::Span;
use diagnostic::{DiagnosticMessage, Label, Note, Severity};
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct DeprecationWarning {
    item: String,
    notes: Vec<Note>,
    span: Option<Span>,
}

impl DeprecationWarning {
    #[must_use]
    pub fn new(item: &str) -> Self {
        DeprecationWarning {
            item: item.to_string(),
            notes: vec![],
            span: None,
        }
    }

    #[must_use]
    pub fn with_note(mut self, note: Note) -> Self {
        self.notes.push(note);
        self
    }

    #[must_use]
    pub fn with_notes(mut self, mut notes: Vec<Note>) -> Self {
        self.notes.append(&mut notes);
        self
    }

    #[must_use]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

impl std::error::Error for DeprecationWarning {}

impl Display for DeprecationWarning {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl DiagnosticMessage for DeprecationWarning {
    fn code(&self) -> usize {
        801
    }

    fn message(&self) -> String {
        format!("{} is deprecated", self.item)
    }

    fn labels(&self) -> Vec<Label> {
        if let Some(span) = self.span {
            vec![Label::primary("this is deprecated", span)]
        } else {
            vec![]
        }
    }

    fn notes(&self) -> Vec<Note> {
        self.notes.clone()
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }
}
