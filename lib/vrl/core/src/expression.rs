use diagnostic::{Diagnostic, DiagnosticMessage, Label, Note, Severity};
use value::Value;

pub type Resolved = Result<Value, ExpressionError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionError {
    #[cfg(feature = "expr-abort")]
    Abort {
        span: diagnostic::Span,
        message: Option<String>,
    },
    Error {
        message: String,
        labels: Vec<Label>,
        notes: Vec<Note>,
    },
}

impl std::fmt::Display for ExpressionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.message().fmt(f)
    }
}

impl std::error::Error for ExpressionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

impl From<ExpressionError> for Diagnostic {
    fn from(error: ExpressionError) -> Self {
        Self {
            severity: Severity::Error,
            code: error.code(),
            message: error.message(),
            labels: error.labels(),
            notes: error.notes(),
        }
    }
}

impl DiagnosticMessage for ExpressionError {
    fn code(&self) -> usize {
        0
    }

    fn message(&self) -> String {
        use ExpressionError::{Abort, Error};

        match self {
            #[cfg(feature = "expr-abort")]
            Abort { message, .. } => message.clone().unwrap_or_else(|| "aborted".to_owned()),
            Error { message, .. } => message.clone(),
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ExpressionError::{Abort, Error};

        match self {
            #[cfg(feature = "expr-abort")]
            Abort { span, .. } => {
                vec![Label::primary("aborted", span)]
            }
            Error { labels, .. } => labels.clone(),
        }
    }

    fn notes(&self) -> Vec<Note> {
        use ExpressionError::{Abort, Error};

        match self {
            #[cfg(feature = "expr-abort")]
            Abort { .. } => vec![],
            Error { notes, .. } => notes.clone(),
        }
    }
}

impl From<String> for ExpressionError {
    fn from(message: String) -> Self {
        ExpressionError::Error {
            message,
            labels: vec![],
            notes: vec![],
        }
    }
}

impl From<&str> for ExpressionError {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}
