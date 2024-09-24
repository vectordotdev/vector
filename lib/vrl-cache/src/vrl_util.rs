//! Utilities shared between VRL functions.
use vrl::diagnostic::{Label, Span};
use vrl::prelude::*;

#[derive(Debug)]
pub enum Error {
    CachesNotLoaded,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::CachesNotLoaded => write!(f, "VRL caches not loaded"),
        }
    }
}

impl std::error::Error for Error {}

impl DiagnosticMessage for Error {
    fn code(&self) -> usize {
        111
    }

    fn labels(&self) -> Vec<Label> {
        match self {
            Error::CachesNotLoaded => {
                vec![Label::primary(
                    "VRL cache error: not loaded".to_string(),
                    Span::default(),
                )]
            }
        }
    }
}
