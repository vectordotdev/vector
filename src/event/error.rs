use crate::event::Value;
use lookup::LookupBuf;
use snafu::Snafu;
pub use EventError::*;

#[derive(Debug, Snafu)]
pub enum EventError {
    #[snafu(display(
        "Cannot insert value nested inside primitive located at {}. {} was the original target.",
        primitive_at,
        original_target
    ))]
    PrimitiveDescent {
        primitive_at: LookupBuf,
        original_target: LookupBuf,
        original_value: Option<Value>,
    },
    #[snafu(display("Lookup Error: {}", source))]
    LookupError { source: lookup::LookupError },
    #[snafu(display("Empty coalesce subsegment found."))]
    EmptyCoalesceSubSegment,
    #[snafu(display("Cannot remove self."))]
    RemovingSelf,
}

impl From<lookup::LookupError> for EventError {
    fn from(v: lookup::LookupError) -> Self {
        Self::LookupError { source: v }
    }
}
