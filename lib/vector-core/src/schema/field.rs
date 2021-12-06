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
