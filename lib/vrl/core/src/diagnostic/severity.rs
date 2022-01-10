use codespan_reporting::diagnostic;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Severity {
    Bug,
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Returns `true` if the severity is a [bug](Severity::Bug).
    #[inline]
    pub fn is_bug(self) -> bool {
        matches!(self, Severity::Bug)
    }

    /// Returns `true` if the severity is an [error](Severity::Error).
    #[inline]
    pub fn is_error(self) -> bool {
        matches!(self, Severity::Error)
    }

    /// Returns `true` if the severity is a [warning](Severity::Warning).
    #[inline]
    pub fn is_warning(self) -> bool {
        matches!(self, Severity::Warning)
    }

    /// Returns `true` if the severity is a [note](Severity::Note).
    #[inline]
    pub fn is_note(self) -> bool {
        matches!(self, Severity::Note)
    }
}

impl From<Severity> for diagnostic::Severity {
    fn from(severity: Severity) -> Self {
        use Severity::*;

        match severity {
            Bug => diagnostic::Severity::Bug,
            Error => diagnostic::Severity::Error,
            Warning => diagnostic::Severity::Warning,
            Note => diagnostic::Severity::Note,
        }
    }
}
