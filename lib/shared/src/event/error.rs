use crate::{lookup::*, event::*};
use std::fmt;
pub use EventError::*;

#[derive(Debug)]
pub enum EventError {
    PrimitiveDescent { primitive_at: LookupBuf, original_target: LookupBuf, original_value: Option<Value>, },
    LookupError(crate::lookup::LookupError),
    EmptyCoalesceSubSegment,
    RemovingSelf,
}

impl fmt::Display for EventError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveDescent { primitive_at, original_target, .. } => write!(
                f,
                "Cannot insert value nested inside primitive located at {}. {} was the original target.",
                primitive_at,
                original_target,
            ),
            LookupError(e) => write!(f, "Lookup Error: {:?}", e,),
            EmptyCoalesceSubSegment => write!(f, "Empty coalesce subsegment found."),
            RemovingSelf =>  write!(f, "Cannot remove self."),
        }
    }
}

impl std::error::Error for EventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PrimitiveDescent { .. } => None,
            LookupError(source) => Some(source),
            EmptyCoalesceSubSegment { .. } => None,
            RemovingSelf => None,
        }
    }
}

impl From<crate::lookup::LookupError> for EventError {
    fn from(v: crate::lookup::LookupError) -> Self {
        Self::LookupError(v)
    }
}
