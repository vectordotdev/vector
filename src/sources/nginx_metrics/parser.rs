use std::convert::TryFrom;

use nom::{
    bytes::complete::{tag, take_while_m_n},
    combinator::{all_consuming, map_res},
    error::ErrorKind,
    sequence::{preceded, terminated, tuple},
};
use snafu::Snafu;

#[derive(Debug, Snafu, PartialEq, Eq)]
pub enum ParseError {
    #[snafu(display("failed to parse NginxStubStatus, kind: `{:?}`", kind))]
    NginxStubStatusParseError { kind: ErrorKind },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NginxStubStatus {
    pub active: usize,
    pub accepts: usize,
    pub handled: usize,
    pub requests: usize,
    pub reading: usize,
    pub writing: usize,
    pub waiting: usize,
}

fn get_usize(input: &str) -> nom::IResult<&str, usize, nom::error::Error<&str>> {
    map_res(
        take_while_m_n(1, 20, |c: char| c.is_ascii_digit()),
        |s: &str| s.parse::<usize>(),
    )(input)
}

impl<'a> TryFrom<&'a str> for NginxStubStatus {
    type Error = ParseError;

    // The `ngx_http_stub_status_module` response:
    // https://github.com/nginx/nginx/blob/master/src/http/modules/ngx_http_stub_status_module.c#L137-L145
    fn try_from(input: &'a str) -> Result<Self, Self::Error> {
        // `usize::MAX` eq `18446744073709551615` (20 characters)
        match all_consuming(tuple((
            preceded(tag("Active connections: "), get_usize),
            preceded(tag(" \nserver accepts handled requests\n "), get_usize),
            preceded(tag(" "), get_usize),
            preceded(tag(" "), get_usize),
            preceded(tag(" \nReading: "), get_usize),
            preceded(tag(" Writing: "), get_usize),
            terminated(preceded(tag(" Waiting: "), get_usize), tag(" \n")),
        )))(input)
        {
            Ok((_, (active, accepts, handled, requests, reading, writing, waiting))) => {
                Ok(NginxStubStatus {
                    active,
                    accepts,
                    handled,
                    requests,
                    reading,
                    writing,
                    waiting,
                })
            }
            Err(error) => match error {
                nom::Err::Error(error) => {
                    Err(ParseError::NginxStubStatusParseError { kind: error.code })
                }
                nom::Err::Incomplete(_) | nom::Err::Failure(_) => unreachable!(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nginx_stub_status_try_from() {
        let data = "Active connections: 291 \n\
                    server accepts handled requests\n \
                    16630948 16630948 31070465 \n\
                    Reading: 6 Writing: 179 Waiting: 106 \n";

        assert_eq!(
            NginxStubStatus::try_from(data).expect("valid data"),
            NginxStubStatus {
                active: 291,
                accepts: 16630948,
                handled: 16630948,
                requests: 31070465,
                reading: 6,
                writing: 179,
                waiting: 106
            }
        );
    }
}
