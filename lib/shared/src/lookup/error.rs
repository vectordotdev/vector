use std::fmt;
use remap_lang::parser::ParserRule;
use std::num::ParseIntError;
pub use LookupError::*;
use pest::error::Error;

#[derive(Debug)]
pub enum LookupError {
    WrongRule {
        wants: &'static [ParserRule],
        got: ParserRule,
    },
    MissingIndex,
    IndexParsing(ParseIntError),
    MissingInnerSegment,
    NoTokens,
    PestParser(pest::error::Error<ParserRule>),
}

impl fmt::Display for LookupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WrongRule { wants, got } => write!(f,
                "Got invalid lookup rule. Got: {:?}. Want: {:?}",
                got,
                wants,
            ),
            MissingIndex => write!(f, "Expected array index, did not get one."),
            IndexParsing(e) => write!(f, "Array index parsing error: {:?}", e),
            MissingInnerSegment => write!(f, "Missing inner of quoted segment."),
            NoTokens => write!(f, "No tokens found to parse."),
            PestParser(e) => write!(f, "Parsing error: {:?}", e),
        }

    }
}

impl std::error::Error for LookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WrongRule { .. } => None,
            MissingIndex => None,
            IndexParsing(e) => Some(e),
            MissingInnerSegment => None,
            NoTokens => None,
            PestParser(e) => Some(e),
        }
    }
}

impl From<pest::error::Error<ParserRule>> for LookupError {
    fn from(v: pest::error::Error<ParserRule>) -> Self {
        Self::PestParser(v)
    }
}
