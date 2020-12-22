use crate::lookup::*;
use std::fmt;
pub use EventError::*;

#[derive(Debug)]
pub enum EventError {
    PrimitiveDescent { location: LookupBuf },
    LookupError(crate::lookup::LookupError),
    EmptyCoalesceSubSegment,
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveDescent { location } => write!(
                f,
                "Cannot insert value nested inside primitive located at {}",
                location,
            ),
            LookupError(e) => write!(f, "Lookup Error: {:?}", e,),
            EmptyCoalesceSubSegment => write!(f, "Empty coalesce subsegment found."),
        }
    }
}

impl std::error::Error for EventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PrimitiveDescent { .. } => None,
            LookupError(source) => Some(source),
            EmptyCoalesceSubSegment { .. } => None,
        }
    }
}

impl From<crate::lookup::LookupError> for EventError {
    fn from(v: crate::lookup::LookupError) -> Self {
        Self::LookupError(v)
    }
}
