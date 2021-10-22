use crate::{
    codecs::{BoxedParser, Parser, ParserConfig},
    config::log_schema,
    event::{Event, LogEvent},
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use smallvec::{smallvec, SmallVec};

/// Config used to build a `BytesParser`.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct BytesParserConfig;

impl BytesParserConfig {
    /// Creates a new `BytesParserConfig`.
    pub const fn new() -> Self {
        Self
    }
}

#[typetag::serde(name = "bytes")]
impl ParserConfig for BytesParserConfig {
    fn build(&self) -> crate::Result<BoxedParser> {
        Ok(Box::new(BytesParser))
    }
}

/// Parser that converts bytes to an `Event`.
///
/// This parser can be considered as the no-op action for input where no further
/// decoding has been specified.
#[derive(Debug, Clone)]
pub struct BytesParser;

impl BytesParser {
    /// Creates a new `BytesParser`.
    pub const fn new() -> Self {
        Self
    }
}

impl Parser for BytesParser {
    fn parse(&self, bytes: Bytes) -> crate::Result<SmallVec<[Event; 1]>> {
        let mut log = LogEvent::default();
        log.insert(log_schema().message_key(), bytes);
        Ok(smallvec![log.into()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::log_schema;

    #[test]
    fn parse_bytes() {
        let input = Bytes::from("foo");
        let parser = BytesParser;

        let events = parser.parse(input).unwrap();
        let mut events = events.into_iter();

        {
            let event = events.next().unwrap();
            let log = event.as_log();
            assert_eq!(log[log_schema().message_key()], "foo".into());
        }

        assert_eq!(events.next(), None);
    }
}
