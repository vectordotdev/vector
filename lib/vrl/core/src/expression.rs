use diagnostic::{Diagnostic, DiagnosticMessage, Label, Note, Severity, Span};
use value::Value;

pub type Resolved = Result<Value, ExpressionError>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExpressionError {
    Abort {
        message: String,
        labels: Vec<Label>,
        notes: Vec<Note>,
    },
    Error {
        message: String,
        labels: Vec<Label>,
        notes: Vec<Note>,
    },
}

impl ExpressionError {
    #[must_use]
    pub fn abort(span: Span, message: Option<&str>) -> ExpressionError {
        let message = if let Some(message) = message {
            format!("explicit abort at {span}: {message}")
        } else {
            format!("explicit abort at {span}")
        };

        ExpressionError::Abort {
            message,
            labels: vec![Label::primary("aborted", span)],
            notes: vec![],
        }
    }

    #[must_use]
    pub fn function_abort(
        span: Span,
        ident: &str,
        abort_on_error: bool,
        error: ExpressionError,
    ) -> ExpressionError {
        let abort = matches!(error, ExpressionError::Abort { .. }) || abort_on_error;
        let (message, labels, notes) = match error {
            ExpressionError::Abort {
                message,
                mut labels,
                notes,
            }
            | ExpressionError::Error {
                message,
                mut labels,
                notes,
            } => {
                let formatted_message = format!(
                    r#"function call error for "{}" at {}: {}"#,
                    ident, span, message
                );
                labels.push(Label::primary(message, span));
                (formatted_message, labels, notes)
            }
        };

        if abort {
            ExpressionError::Abort {
                message,
                labels,
                notes,
            }
        } else {
            ExpressionError::Error {
                message,
                labels,
                notes,
            }
        }
    }

    #[must_use]
    pub fn is_abort(&self) -> bool {
        matches!(self, ExpressionError::Abort { .. })
    }
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
            Abort { message, .. } | Error { message, .. } => message.clone(),
        }
    }

    fn labels(&self) -> Vec<Label> {
        use ExpressionError::{Abort, Error};

        match self {
            Abort { labels, .. } | Error { labels, .. } => labels.clone(),
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
