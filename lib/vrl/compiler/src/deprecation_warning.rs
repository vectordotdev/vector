use crate::Span;
use diagnostic::{DiagnosticMessage, Label, Note, Severity};
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct DeprecationWarning {
    item: String,
    alternatives: Vec<String>,
    span: Option<Span>,
}

impl DeprecationWarning {
    #[must_use]
    pub fn new(item: &str) -> Self {
        DeprecationWarning {
            item: item.to_string(),
            alternatives: vec![],
            span: None,
        }
    }

    #[must_use]
    pub fn with_alternative(mut self, alternative: &str) -> Self {
        self.alternatives.push(alternative.to_string());
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
        9999
    }

    fn message(&self) -> String {
        format!("{} is deprecated.", self.item)
    }

    fn labels(&self) -> Vec<Label> {
        if let Some(span) = self.span {
            vec![Label::primary("this is deprecated", span)]
        } else {
            vec![]
        }
    }

    fn notes(&self) -> Vec<Note> {
        self.alternatives
            .iter()
            .map(|alternative| Note::Hint(alternative.to_string()))
            .collect()
    }

    fn severity(&self) -> Severity {
        Severity::Warning
    }
}
