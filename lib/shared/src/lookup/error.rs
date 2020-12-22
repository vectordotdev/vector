use std::fmt;
use remap_lang::parser::ParserRule;
use std::num::ParseIntError;
pub use LookupError::*;

#[derive(Debug)]
pub enum LookupError {
    WrongRule {
        wants: &'static [ParserRule],
        got: ParserRule,
    },
    MissingIndex,
    IndexParsing(ParseIntError),
    MissingInnerSegment,
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
        }

    }
}

impl std::error::Error for LookupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            WrongRule { .. } => None,
            MissingIndex => None,
            IndexParsing(inner) => Some(inner),
            MissingInnerSegment => None,
        }
    }
}
