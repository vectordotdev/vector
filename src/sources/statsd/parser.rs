use crate::event::{metric::Direction, Metric};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    error, fmt,
    num::{ParseFloatError, ParseIntError},
};

lazy_static! {
    static ref WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
    static ref NONALPHANUM: Regex = Regex::new(r"[^a-zA-Z_\-0-9\.]").unwrap();
}

pub fn parse(packet: &str) -> Result<Metric, ParseError> {
    let key_and_body = packet.splitn(2, ":").collect::<Vec<_>>();
    if key_and_body.len() != 2 {
        return Err(ParseError::Malformed(
            "should be key and body with ':' separator",
        ));
    }
    let (key, body) = (key_and_body[0], key_and_body[1]);

    let parts = body.split("|").collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(ParseError::Malformed(
            "body should have at least two pipe separated components",
        ));
    }
    let metric_type = parts[1];

    let metric = match metric_type {
        "c" => {
            let count = if let Some(s) = parts.get(2) {
                1.0 / sanitize_sampling(parse_sampling(s)?)
            } else {
                1.0
            };
            let val: f32 = parts[0].parse()?;
            let metric = Metric::Counter {
                name: sanitize_key(key),
                val: val * count,
            };
            metric
        }
        "h" | "ms" => {
            let count = if let Some(s) = parts.get(2) {
                1.0 / sanitize_sampling(parse_sampling(s)?)
            } else {
                1.0
            };
            let val: f32 = parts[0].parse()?;
            let metric = Metric::Timer {
                name: sanitize_key(key),
                val: val * count,
                count,
            };
            metric
        }
        "g" => {
            let val = if parts[0]
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .ok_or_else(|| ParseError::Malformed("empty first body component"))?
            {
                parts[0].parse()?
            } else {
                parts[0][1..].parse()?
            };
            let metric = Metric::Gauge {
                name: sanitize_key(key),
                val,
                direction: parse_direction(parts[0])?,
            };
            metric
        }
        "s" => {
            let metric = Metric::Set {
                name: sanitize_key(key),
                val: parts[0].into(),
            };
            metric
        }
        other => return Err(ParseError::UnknownMetricType(other.into())),
    };
    Ok(metric)
}

fn parse_sampling(input: &str) -> Result<f32, ParseError> {
    if input.chars().next() != Some('@') || input.len() < 2 {
        return Err(ParseError::Malformed(
            "expected '@'-prefixed sampling component",
        ));
    }

    let num: f32 = input[1..].parse()?;
    if num.is_sign_positive() {
        Ok(num)
    } else {
        Err(ParseError::Malformed("sample rate can't be negative"))
    }
}

fn parse_direction(input: &str) -> Result<Option<Direction>, ParseError> {
    match input
        .chars()
        .next()
        .ok_or_else(|| ParseError::Malformed("empty body component"))?
    {
        '+' => Ok(Some(Direction::Plus)),
        '-' => Ok(Some(Direction::Minus)),
        c if c.is_ascii_digit() => Ok(None),
        _other => Err(ParseError::Malformed("invalid gauge value prefix")),
    }
}

fn sanitize_key(key: &str) -> String {
    let s = key.replace("/", "-");
    let s = WHITESPACE.replace_all(&s, "_");
    let s = NONALPHANUM.replace_all(&s, "");
    s.into()
}

fn sanitize_sampling(sampling: f32) -> f32 {
    if sampling == 0.0 {
        1.0
    } else {
        sampling
    }
}

#[derive(Debug, PartialEq)]
pub enum ParseError {
    Malformed(&'static str),
    UnknownMetricType(String),
    InvalidInteger(ParseIntError),
    InvalidFloat(ParseFloatError),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            anything => write!(f, "Statsd parse error: {:?}", anything),
        }
    }
}

impl error::Error for ParseError {}

impl From<ParseIntError> for ParseError {
    fn from(e: ParseIntError) -> ParseError {
        ParseError::InvalidInteger(e)
    }
}

impl From<ParseFloatError> for ParseError {
    fn from(e: ParseFloatError) -> ParseError {
        ParseError::InvalidFloat(e)
    }
}

#[cfg(test)]
mod test {
    use super::{parse, sanitize_key};
    use crate::event::{metric::Direction, Metric};

    #[test]
    fn basic_counter() {
        assert_eq!(
            parse("foo:1|c"),
            Ok(Metric::Counter {
                name: "foo".into(),
                val: 1.0,
            }),
        );
    }

    #[test]
    fn sampled_counter() {
        assert_eq!(
            parse("bar:2|c|@0.1"),
            Ok(Metric::Counter {
                name: "bar".into(),
                val: 20.0,
            }),
        );
    }

    #[test]
    fn zero_sampled_counter() {
        assert_eq!(
            parse("bar:2|c|@0"),
            Ok(Metric::Counter {
                name: "bar".into(),
                val: 2.0,
            }),
        );
    }

    #[test]
    fn timer() {
        assert_eq!(
            parse("glork:320|ms|@0.1"),
            Ok(Metric::Timer {
                name: "glork".into(),
                val: 3200.0,
                count: 10.0
            }),
        );
    }

    #[test]
    fn simple_gauge() {
        assert_eq!(
            parse("gaugor:333|g"),
            Ok(Metric::Gauge {
                name: "gaugor".into(),
                val: 333.0,
                direction: None
            }),
        );
    }

    #[test]
    fn signed_gauge() {
        assert_eq!(
            parse("gaugor:-4|g"),
            Ok(Metric::Gauge {
                name: "gaugor".into(),
                val: 4.0,
                direction: Some(Direction::Minus)
            }),
        );
        assert_eq!(
            parse("gaugor:+10|g"),
            Ok(Metric::Gauge {
                name: "gaugor".into(),
                val: 10.0,
                direction: Some(Direction::Plus)
            }),
        );
    }

    #[test]
    fn sets() {
        assert_eq!(
            parse("uniques:765|s"),
            Ok(Metric::Set {
                name: "uniques".into(),
                val: "765".into(),
            }),
        );
    }

    #[test]
    fn sanitizing_keys() {
        assert_eq!("foo-bar-baz", sanitize_key("foo/bar/baz"));
        assert_eq!("foo_bar_baz", sanitize_key("foo bar  baz"));
        assert_eq!("foo.__bar_.baz", sanitize_key("foo. @& bar_$!#.baz"));
    }
}
