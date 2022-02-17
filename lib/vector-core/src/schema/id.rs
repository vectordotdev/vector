use std::num::NonZeroU16;

use serde::{Deserialize, Serialize};

/// An identifier to a globally configured event schema.
///
/// A maximum of `65_535` schemas are supported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(transparent)]
pub struct Id(u16);

impl Id {
    /// The "empty" ID represents a default schema that contains no knowledge about the underlying
    /// data.
    pub fn empty() -> Self {
        Self(0)
    }
}

impl From<NonZeroU16> for Id {
    fn from(id: NonZeroU16) -> Self {
        Self(id.get())
    }
}
