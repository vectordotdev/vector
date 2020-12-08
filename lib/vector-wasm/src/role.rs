use serde::{Deserialize, Serialize};

/// Denotes the intended role of the module.
///
/// This type is used as part of the [`Registration`](guest::Registration) process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(C)]
pub enum Role {
    /// A transform.
    Transform = 0,
    /// A source.
    Source = 1,
    /// A sink.
    Sink = 2,
}

impl Role {
    /// Cheaply turn into a `&'static str` so you don't need to format it for metrics.
    pub fn as_const_str(self) -> &'static str {
        match self {
            Role::Transform => TRANSFORM,
            Role::Source => SOURCE,
            Role::Sink => SINK,
        }
    }
}

pub const TRANSFORM: &str = "transform";
pub const SOURCE: &str = "source";
pub const SINK: &str = "sink";
