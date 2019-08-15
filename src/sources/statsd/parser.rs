use crate::event::{metric::Direction, Metric};
use lazy_static::lazy_static;
use regex::Regex;
use std::{
    collections::HashMap,
    error, fmt,
    num::{ParseFloatError, ParseIntError},
};

lazy_static! {
    static ref WHITESPACE: Regex = Regex::new(r"\s+").unwrap();
    static ref NONALPHANUM: Regex = Regex::new(r"[^a-zA-Z_\-0-9\.]").unwrap();
}

pub fn parse(packet: &str) -> Result<Metric, ParseError> {
    // https://docs.datadoghq.com/developers/dogstatsd/datagram_shell/#datagram-format
    let key_and_body = packet.splitn(2, ':').collect::<Vec<_>>();
    if key_and_body.len() != 2 {
        return Err(ParseError::Malformed(
            "should be key and body with ':' separator",
        ));
    }
    let (key, body) = (key_and_body[0], key_and_body[1]);

    let parts = body.split('|').collect::<Vec<_>>();
    if parts.len() < 2 {
        return Err(ParseError::Malformed(
            "body should have at least two pipe separated components",
        ));
    }

    let name = sanitize_key(key);
    let metric_type = parts[1];

    // sampling part is optional and comes after metric type part
    let sampling = parts.get(2).filter(|s| s.starts_with('@'));
    let sample_rate = if let Some(s) = sampling {
        1.0 / sanitize_sampling(parse_sampling(s)?)
    } else {
        1.0
    };

    // tags are optional and could be found either after sampling of after metric type part
    let tags = if sampling.is_none() {
        parts.get(2)
    } else {
        parts.get(3)
    };
    let tags = tags.filter(|s| s.starts_with('#'));
    let tags = if let Some(t) = tags {
        Some(parse_tags(t)?)
    } else {
        None
    };

    let metric = match metric_type {
        "c" => {
            let val: f64 = parts[0].parse()?;
            Metric::Counter {
                name,
                val: val * sample_rate,
                timestamp: None,
                tags,
            }
        }
        unit @ "h" | unit @ "ms" => {
            let val: f64 = parts[0].parse()?;
            Metric::Histogram {
                name,
                val: convert_to_base_units(unit, val),
                sample_rate: sample_rate as u32,
                timestamp: None,
                tags,
            }
        }
        "g" => Metric::Gauge {
            name,
            val: if parts[0]
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .ok_or_else(|| ParseError::Malformed("empty first body component"))?
            {
                parts[0].parse()?
            } else {
                parts[0][1..].parse()?
            },
            direction: parse_direction(parts[0])?,
            timestamp: None,
            tags,
        },
        "s" => Metric::Set {
            name,
            val: parts[0].into(),
            timestamp: None,
            tags,
        },
        other => return Err(ParseError::UnknownMetricType(other.into())),
    };
    Ok(metric)
}

fn parse_sampling(input: &str) -> Result<f64, ParseError> {
    if !input.starts_with('@') || input.len() < 2 {
        return Err(ParseError::Malformed(
            "expected non empty '@'-prefixed sampling component",
        ));
    }

    let num: f64 = input[1..].parse()?;
    if num.is_sign_positive() {
        Ok(num)
    } else {
        Err(ParseError::Malformed("sample rate can't be negative"))
    }
}

fn parse_tags(input: &str) -> Result<HashMap<String, String>, ParseError> {
    if !input.starts_with('#') || input.len() < 2 {
        return Err(ParseError::Malformed(
            "expected non empty '#'-prefixed tags component",
        ));
    }

    let mut result = HashMap::new();

    let chunks = input[1..].split(',').collect::<Vec<_>>();
    for chunk in chunks {
        let pair: Vec<_> = chunk.split(':').collect();
        let key = &pair[0];
        // same as in telegraf plugin:
        // if tag value is not provided, use "true"
        // https://github.com/influxdata/telegraf/blob/master/plugins/inputs/statsd/datadog.go#L152
        let value = pair.get(1).unwrap_or(&"true");
        result.insert((*key).to_owned(), (*value).to_owned());
    }

    Ok(result)
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

fn sanitize_sampling(sampling: f64) -> f64 {
    if sampling == 0.0 {
        1.0
    } else {
        sampling
    }
}

fn convert_to_base_units(unit: &str, val: f64) -> f64 {
    match unit {
        "ms" => val / 1000.0,
        _ => val,
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    use super::{parse, sanitize_key, sanitize_sampling};
    use crate::event::{metric::Direction, Metric};

    #[test]
    fn basic_counter() {
        assert_eq!(
            parse("foo:1|c"),
            Ok(Metric::Counter {
                name: "foo".into(),
                val: 1.0,
                timestamp: None,
                tags: None,
            }),
        );
    }

    #[test]
    fn tagged_counter() {
        assert_eq!(
            parse("foo:1|c|#tag1,tag2:value"),
            Ok(Metric::Counter {
                name: "foo".into(),
                val: 1.0,
                timestamp: None,
                tags: Some(
                    vec![
                        ("tag1".to_owned(), "true".to_owned()),
                        ("tag2".to_owned(), "value".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
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
                timestamp: None,
                tags: None,
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
                timestamp: None,
                tags: None,
            }),
        );
    }

    #[test]
    fn sampled_timer() {
        assert_eq!(
            parse("glork:320|ms|@0.1"),
            Ok(Metric::Histogram {
                name: "glork".into(),
                val: 0.320,
                sample_rate: 10,
                timestamp: None,
                tags: None,
            }),
        );
    }

    #[test]
    fn sampled_tagged_histogram() {
        assert_eq!(
            parse("glork:320|h|@0.1|#region:us-west1,production,e:"),
            Ok(Metric::Histogram {
                name: "glork".into(),
                val: 320.0,
                sample_rate: 10,
                timestamp: None,
                tags: Some(
                    vec![
                        ("region".to_owned(), "us-west1".to_owned()),
                        ("production".to_owned(), "true".to_owned()),
                        ("e".to_owned(), "".to_owned()),
                    ]
                    .into_iter()
                    .collect(),
                ),
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
                direction: None,
                timestamp: None,
                tags: None,
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
                direction: Some(Direction::Minus),
                timestamp: None,
                tags: None,
            }),
        );
        assert_eq!(
            parse("gaugor:+10|g"),
            Ok(Metric::Gauge {
                name: "gaugor".into(),
                val: 10.0,
                direction: Some(Direction::Plus),
                timestamp: None,
                tags: None,
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
                timestamp: None,
                tags: None,
            }),
        );
    }

    #[test]
    fn sanitizing_keys() {
        assert_eq!("foo-bar-baz", sanitize_key("foo/bar/baz"));
        assert_eq!("foo_bar_baz", sanitize_key("foo bar  baz"));
        assert_eq!("foo.__bar_.baz", sanitize_key("foo. @& bar_$!#.baz"));
    }

    #[test]
    fn sanitizing_sampling() {
        assert_eq!(1.0, sanitize_sampling(0.0));
        assert_eq!(2.5, sanitize_sampling(2.5));
        assert_eq!(-5.0, sanitize_sampling(-5.0));
    }
}
