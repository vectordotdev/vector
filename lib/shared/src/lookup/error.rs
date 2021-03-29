// use remap_lang::parser::ParserRule;
use snafu::Snafu;
use std::num::ParseIntError;
pub use LookupError::*;

#[derive(Debug, Snafu)]
pub enum LookupError {
    #[snafu(display("Got invalid lookup rule. Got: {:?}, Want: {:?}.", wants, got))]
    WrongRule {
        wants: &'static [ParserRule],
        got: ParserRule,
    },
    #[snafu(display("Expected array index, did not get one."))]
    MissingIndex,
    #[snafu(display("Array index parsing error: {}", source))]
    IndexParsing { source: ParseIntError },
    #[snafu(display("Missing inner of quoted segment."))]
    MissingInnerSegment,
    #[snafu(display("No tokens found to parse."))]
    NoTokens,
    /*
    #[snafu(display("Parsing error: {}", source))]
    PestParser {
        source: pest::error::Error<ParserRule>,
    },
    */
}

/*
impl From<pest::error::Error<ParserRule>> for LookupError {
    fn from(source: pest::error::Error<ParserRule>) -> Self {
        Self::PestParser { source }
    }
}
*/
