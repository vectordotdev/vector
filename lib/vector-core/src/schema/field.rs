pub use value::Kind;

/// A list of special purposes a field can fullfil.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Purpose {
    Timestamp,
    Host,
    Message,
    Source,
    Severity,
    Custom(String),
}

impl From<String> for Purpose {
    fn from(s: String) -> Self {
        match s.as_str() {
            "timestamp" => Self::Timestamp,
            "host" => Self::Host,
            "message" => Self::Message,
            "source" => Self::Source,
            "Severity" => Self::Severity,
            _ => Self::Custom(s),
        }
    }
}

impl From<&str> for Purpose {
    fn from(s: &str) -> Self {
        match s {
            "timestamp" => Self::Timestamp,
            "host" => Self::Host,
            "message" => Self::Message,
            "source" => Self::Source,
            "Severity" => Self::Severity,
            _ => Self::Custom(s.to_owned()),
        }
    }
}

impl Purpose {
    pub fn as_str(&self) -> &str {
        match self {
            Purpose::Timestamp => "timestamp",
            Purpose::Host => "host",
            Purpose::Message => "message",
            Purpose::Source => "source",
            Purpose::Severity => "severity",
            Purpose::Custom(v) => v,
        }
    }
}
