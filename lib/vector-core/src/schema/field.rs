pub use value::Kind;

/// A list of special purposes a field can fullfil.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum Purpose {
    Timestamp,
    Host,
    Message,
    Source,
    Severity,
    Custom(&'static str),
}

impl From<&'static str> for Purpose {
    fn from(s: &'static str) -> Self {
        match s {
            "timestamp" => Self::Timestamp,
            "host" => Self::Host,
            "message" => Self::Message,
            "source" => Self::Source,
            "Severity" => Self::Severity,
            _ => Self::Custom(s),
        }
    }
}
