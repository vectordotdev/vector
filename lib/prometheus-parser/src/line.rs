//! Parse a single line of Prometheus text format.

use std::collections::BTreeMap;

use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_while, take_while1},
    character::complete::{char, digit1},
    combinator::{map, opt, recognize, value},
    error::ParseError,
    multi::fold_many0,
    number::complete::double,
    sequence::{delimited, pair, preceded, tuple},
};

/// We try to catch all nom's `ErrorKind` with our own `ErrorKind`,
/// to provide a meaningful error message.
/// Parsers in this module should return this IResult instead of `nom::IResult`.
type IResult<'a, O> = Result<(&'a str, O), nom::Err<ErrorKind>>;

#[derive(Debug, snafu::Snafu, PartialEq, Eq)]
pub enum ErrorKind {
    #[snafu(display("invalid metric type, parsing: `{}`", input))]
    InvalidMetricKind { input: String },
    #[snafu(display("expected token {:?}, parsing: `{}`", expected, input))]
    ExpectedToken {
        expected: &'static str,
        input: String,
    },
    #[snafu(display("expected blank space or tab, parsing: `{}`", input))]
    ExpectedSpace { input: String },
    #[snafu(display("expected token {:?}, parsing: `{}`", expected, input))]
    ExpectedChar { expected: char, input: String },
    #[snafu(display("name must start with [a-zA-Z_], parsing: `{}`", input))]
    ParseNameError { input: String },
    #[snafu(display("parse float value error, parsing: `{}`", input))]
    ParseFloatError { input: String },
    #[snafu(display("parse timestamp error, parsing: `{}`", input))]
    ParseTimestampError { input: String },

    // Error that we didn't catch
    #[snafu(display("error kind: {:?}, parsing: `{}`", kind, input))]
    Nom {
        input: String,
        kind: nom::error::ErrorKind,
    },
}

impl From<ErrorKind> for nom::Err<ErrorKind> {
    fn from(error: ErrorKind) -> Self {
        nom::Err::Error(error)
    }
}

impl From<nom::Err<ErrorKind>> for ErrorKind {
    fn from(error: nom::Err<ErrorKind>) -> Self {
        match error {
            // this error only occurs when "streaming" nom is used.
            nom::Err::Incomplete(_) => unreachable!(),
            nom::Err::Error(e) | nom::Err::Failure(e) => e,
        }
    }
}

impl<'a> nom::error::ParseError<&'a str> for ErrorKind {
    fn from_error_kind(input: &str, kind: nom::error::ErrorKind) -> Self {
        ErrorKind::Nom {
            input: input.to_owned(),
            kind,
        }
    }

    fn append(_: &str, _: nom::error::ErrorKind, other: Self) -> Self {
        other
    }
}

type NomErrorType<'a> = (&'a str, nom::error::ErrorKind);

type NomError<'a> = nom::Err<NomErrorType<'a>>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
    Summary,
    Untyped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub metric_name: String,
    pub kind: MetricKind,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub name: String,
    pub labels: BTreeMap<String, String>,
    pub value: f64,
    pub timestamp: Option<i64>,
}

impl Metric {
    /// Parse a single line with format
    ///
    /// ``` text
    /// metric_name [
    ///   "{" label_name "=" `"` label_value `"` { "," label_name "=" `"` label_value `"` } [ "," ] "}"
    /// ] value [ timestamp ]
    /// ```
    ///
    /// We don't parse timestamp.
    fn parse(input: &str) -> IResult<Self> {
        let input = trim_space(input);
        let (input, name) = parse_name(input)?;
        let (input, labels) = Self::parse_labels(input)?;
        let (input, value) = Self::parse_value(input)?;
        let (input, timestamp) = Self::parse_timestamp(input)?;
        Ok((
            input,
            Metric {
                name,
                labels,
                value,
                timestamp,
            },
        ))
    }

    /// Float value, and +Inf, -Int, Nan.
    pub(crate) fn parse_value(input: &str) -> IResult<f64> {
        let input = trim_space(input);
        alt((
            value(f64::INFINITY, tag("+Inf")),
            value(f64::NEG_INFINITY, tag("-Inf")),
            value(f64::NAN, tag("Nan")),
            // Note see https://github.com/Geal/nom/issues/1384
            // This shouldn't be necessary if that issue is remedied.
            value(f64::NAN, tag("NaN")),
            double,
        ))(input)
        .map_err(|_: NomError| {
            ErrorKind::ParseFloatError {
                input: input.to_owned(),
            }
            .into()
        })
    }

    fn parse_timestamp(input: &str) -> IResult<Option<i64>> {
        let input = trim_space(input);
        opt(map(recognize(pair(opt(char('-')), digit1)), |s: &str| {
            s.parse().unwrap()
        }))(input)
    }

    fn parse_name_value(input: &str) -> IResult<(String, String)> {
        map(
            tuple((parse_name, match_char('='), Self::parse_escaped_string)),
            |(name, _, value)| (name, value),
        )(input)
    }

    // Return:
    // - Some((name, value)) => success
    // - None => list is properly ended with "}"
    // - Error => errors of parse_name_value
    fn element_parser(input: &str) -> IResult<Option<(String, String)>> {
        match Self::parse_name_value(input) {
            Ok((input, result)) => Ok((input, Some(result))),
            Err(nom::Err::Error(parse_name_value_error)) => match match_char('}')(input) {
                Ok((input, _)) => Ok((input, None)),
                Err(nom::Err::Error(_)) => Err(nom::Err::Error(parse_name_value_error)),
                Err(failure) => Err(failure),
            },
            Err(failure) => Err(failure),
        }
    }

    fn parse_labels_inner(mut input: &str) -> IResult<BTreeMap<String, String>> {
        let sep = match_char(',');

        let mut result = BTreeMap::new();
        loop {
            match Self::element_parser(input)? {
                (inner_input, None) => {
                    input = inner_input;
                    break;
                }
                (inner_input, Some((name, value))) => {
                    result.insert(name, value);

                    // try matching ",", if doesn't match then
                    // check if the list ended with "}".
                    // If not ended then return error `expected token ','`.
                    let inner_input = match sep(inner_input) {
                        Ok((inner_input, _)) => inner_input,
                        Err(sep_err) => match match_char('}')(inner_input) {
                            Ok((inner_input, _)) => {
                                input = inner_input;
                                break;
                            }
                            Err(_) => return Err(sep_err),
                        },
                    };

                    input = inner_input;
                }
            }
        }
        Ok((input, result))
    }

    /// Parse `{label_name="value",...}`
    fn parse_labels(input: &str) -> IResult<BTreeMap<String, String>> {
        let input = trim_space(input);

        match opt(char('{'))(input) {
            Ok((input, None)) => Ok((input, BTreeMap::new())),
            Ok((input, Some(_))) => Self::parse_labels_inner(input),
            Err(failure) => Err(failure),
        }
    }

    /// Parse `'"' string_content '"'`. `string_content` can contain any unicode characters,
    /// backslash (`\`), double-quote (`"`), and line feed (`\n`) characters have to be
    /// escaped as `\\`, `\"`, and `\n`, respectively.
    fn parse_escaped_string(input: &str) -> IResult<String> {
        #[derive(Debug)]
        enum StringFragment<'a> {
            Literal(&'a str),
            EscapedChar(char),
        }

        let parse_string_fragment = alt((
            map(is_not("\"\\"), StringFragment::Literal),
            map(
                preceded(
                    char('\\'),
                    alt((
                        value('\n', char('n')),
                        value('"', char('"')),
                        value('\\', char('\\')),
                    )),
                ),
                StringFragment::EscapedChar,
            ),
        ));

        let input = trim_space(input);

        let build_string = fold_many0(
            parse_string_fragment,
            String::new,
            |mut result, fragment| {
                match fragment {
                    StringFragment::Literal(s) => result.push_str(s),
                    StringFragment::EscapedChar(c) => result.push(c),
                }
                result
            },
        );

        fn match_quote(input: &str) -> IResult<char> {
            char('"')(input).map_err(|_: NomError| {
                ErrorKind::ExpectedChar {
                    expected: '"',
                    input: input.to_owned(),
                }
                .into()
            })
        }

        delimited(match_quote, build_string, match_quote)(input)
    }
}

impl Header {
    fn space1(input: &str) -> IResult<()> {
        take_while1(|c| c == ' ' || c == '\t')(input)
            .map_err(|_: NomError| {
                ErrorKind::ExpectedSpace {
                    input: input.to_owned(),
                }
                .into()
            })
            .map(|(input, _)| (input, ()))
    }

    /// `# TYPE <metric_name> <metric_type>`
    fn parse(input: &str) -> IResult<Self> {
        let input = trim_space(input);
        let (input, _) = char('#')(input).map_err(|_: NomError| ErrorKind::ExpectedChar {
            expected: '#',
            input: input.to_owned(),
        })?;
        let input = trim_space(input);
        let (input, _) = tag("TYPE")(input).map_err(|_: NomError| ErrorKind::ExpectedToken {
            expected: "TYPE",
            input: input.to_owned(),
        })?;
        let (input, _) = Self::space1(input)?;
        let (input, metric_name) = parse_name(input)?;
        let (input, _) = Self::space1(input)?;
        let (input, kind) = alt((
            value(MetricKind::Counter, tag("counter")),
            value(MetricKind::Gauge, tag("gauge")),
            value(MetricKind::Summary, tag("summary")),
            value(MetricKind::Histogram, tag("histogram")),
            value(MetricKind::Untyped, tag("untyped")),
        ))(input)
        .map_err(|_: NomError| ErrorKind::InvalidMetricKind {
            input: input.to_owned(),
        })?;
        Ok((input, Header { metric_name, kind }))
    }
}

/// Each line of Prometheus text format.
/// We discard empty lines, comments, and timestamps.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Header(Header),
    Metric(Metric),
}

impl Line {
    /// Parse a single line. Return `None` if it is a comment or an empty line.
    pub(crate) fn parse(input: &str) -> Result<Option<Self>, ErrorKind> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }

        let metric_error = match Metric::parse(input) {
            Ok((_, metric)) => {
                return Ok(Some(Line::Metric(metric)));
            }
            Err(e) => e.into(),
        };

        let header_error = match Header::parse(input) {
            Ok((_, header)) => {
                return Ok(Some(Line::Header(header)));
            }
            Err(e) => e.into(),
        };

        if let Ok((input, _)) = char::<_, NomErrorType>('#')(input) {
            if tuple::<_, _, NomErrorType, _>((sp, tag("TYPE")))(input).is_ok() {
                return Err(header_error);
            }
            Ok(None)
        } else {
            Err(metric_error)
        }
    }
}

/// Name matches the regex `[a-zA-Z_][a-zA-Z0-9_]*`.
fn parse_name(input: &str) -> IResult<String> {
    let input = trim_space(input);
    let (input, (a, b)) = pair(
        take_while1(|c: char| c.is_alphabetic() || c == '_'),
        take_while(|c: char| c.is_alphanumeric() || c == '_' || c == ':'),
    )(input)
    .map_err(|_: NomError| ErrorKind::ParseNameError {
        input: input.to_owned(),
    })?;
    Ok((input, a.to_owned() + b))
}

fn trim_space(input: &str) -> &str {
    input.trim_start_matches(|c| c == ' ' || c == '\t')
}

fn sp<'a, E: ParseError<&'a str>>(i: &'a str) -> nom::IResult<&'a str, &'a str, E> {
    take_while(|c| c == ' ' || c == '\t')(i)
}

fn match_char(c: char) -> impl Fn(&str) -> IResult<char> {
    move |input| {
        preceded(sp, char(c))(input).map_err(|_: NomError| {
            ErrorKind::ExpectedChar {
                expected: c,
                input: input.to_owned(),
            }
            .into()
        })
    }
}

#[cfg(test)]
mod test {
    use vector_common::btreemap;

    use super::*;

    #[test]
    fn test_parse_escaped_string() {
        fn wrap(s: &str) -> String {
            format!("  \t \"{}\"  .", s)
        }

        // parser should not consume more that it needed
        let tail = "  .";

        let input = wrap("");
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "");

        let input = wrap(r"a\\ asdf");
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "a\\ asdf");

        let input = wrap(r#"\"\""#);
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\"\"");

        let input = wrap(r#"\"\\\n"#);
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\"\\\n");

        let input = wrap(r"\\n");
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\\n");

        let input = wrap(r#"  😂  "#);
        let (left, r) = Metric::parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "  😂  ");
    }

    #[test]
    fn test_parse_name() {
        fn wrap(s: &str) -> String {
            format!("  \t {}  .", s)
        }
        let tail = "  .";

        let input = wrap("abc_def");
        let (left, r) = parse_name(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "abc_def");

        let input = wrap("__9A0bc_def__");
        let (left, r) = parse_name(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "__9A0bc_def__");

        let input = wrap("99");
        assert!(parse_name(&input).is_err());

        let input = wrap("consul_serf_events_consul:new_leader");
        let (left, r) = parse_name(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "consul_serf_events_consul:new_leader");
    }

    #[test]
    fn test_parse_header() {
        fn wrap(s: &str) -> String {
            format!("  \t {}  .", s)
        }
        let tail = "  .";

        let input = wrap("#  TYPE abc_def counter");
        let (left, r) = Header::parse(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Counter,
            }
        );

        // We allow this case
        let input = wrap("#  TYPE abc_def counteraaaaaaaaaaa");
        let (_, r) = Header::parse(&input).unwrap();
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Counter,
            }
        );

        let input = wrap("#TYPE \t abc_def \t gauge");
        let (left, r) = Header::parse(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Gauge,
            }
        );

        let input = wrap("# TYPE abc_def histogram");
        let (left, r) = Header::parse(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Histogram,
            }
        );

        let input = wrap("# TYPE abc_def summary");
        let (left, r) = Header::parse(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Summary,
            }
        );

        let input = wrap("# TYPE abc_def untyped");
        let (left, r) = Header::parse(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(
            r,
            Header {
                metric_name: "abc_def".into(),
                kind: MetricKind::Untyped,
            }
        );
    }

    #[test]
    fn test_parse_value() {
        fn wrap(s: &str) -> String {
            format!("  \t {}  .", s)
        }
        let tail = "  .";

        let input = wrap("+Inf");
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert!(r.is_infinite() && r.is_sign_positive());

        let input = wrap("-Inf");
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert!(r.is_infinite() && r.is_sign_negative());

        let input = wrap("Nan");
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert!(r.is_nan());

        let input = wrap(&(u64::MAX).to_string());
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r as u64, u64::MAX);

        // This doesn't pass because we are parsing as f64 so we lose precision
        // once the number is bigger than 2^53 (the mantissa for f64).
        //
        // let input = wrap(&(u64::MAX - 1).to_string());
        // let (left, r) = Metric::parse_value(&input).unwrap();
        // assert_eq!(left, tail);
        // assert_eq!(r as u64, u64::MAX - 1);

        // Precision is maintained up to 2^53
        let input = wrap(&(2_u64.pow(53)).to_string());
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r as u64, 2_u64.pow(53));

        let input = wrap(&(2_u64.pow(53) - 1).to_string());
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r as u64, 2_u64.pow(53) - 1);

        let input = wrap(&(u32::MAX as u64 + 1).to_string());
        let (left, r) = Metric::parse_value(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r as u64, u32::MAX as u64 + 1);

        let tests = [
            ("0", 0.0f64),
            ("0.25", 0.25f64),
            ("-10.25", -10.25f64),
            ("-10e-25", -10e-25f64),
            ("-10e+25", -10e+25f64),
            ("2020", 2020.0f64),
            ("1.", 1f64),
        ];
        for (text, value) in &tests {
            let input = wrap(text);
            let (left, r) = Metric::parse_value(&input).unwrap();
            assert_eq!(left, tail);
            assert!((r - *value).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn test_parse_labels() {
        fn wrap(s: &str) -> String {
            format!("  \t {}  .", s)
        }
        let tail = "  .";

        let input = wrap("{}");
        let (left, r) = Metric::parse_labels(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, BTreeMap::new());

        let input = wrap(r#"{name="value"}"#);
        let (left, r) = Metric::parse_labels(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, BTreeMap::from([("name".into(), "value".into())]));

        let input = wrap(r#"{name="value",}"#);
        let (left, r) = Metric::parse_labels(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, BTreeMap::from([("name".into(), "value".into())]));

        let input = wrap(r#"{ name = "" ,b="a=b" , a="},", _c = "\""}"#);
        let (left, r) = Metric::parse_labels(&input).unwrap();
        assert_eq!(
            r,
            btreemap! {"name" => "", "a" => "},", "b" => "a=b", "_c" => "\""}
        );
        assert_eq!(left, tail);

        let input = wrap("100");
        let (left, r) = Metric::parse_labels(&input).unwrap();
        assert_eq!(left, "100".to_owned() + tail);
        assert_eq!(r, BTreeMap::new());

        // We don't allow these values

        let input = wrap(r#"{name="value}"#);
        let error = Metric::parse_labels(&input).unwrap_err().into();
        assert!(matches!(
            error,
            ErrorKind::ExpectedChar { expected: '"', .. }
        ));

        let input = wrap(r#"{ a="b" c="d" }"#);
        let error = Metric::parse_labels(&input).unwrap_err().into();
        assert!(matches!(
            error,
            ErrorKind::ExpectedChar { expected: ',', .. }
        ));

        let input = wrap(r#"{ a="b" ,, c="d" }"#);
        let error = Metric::parse_labels(&input).unwrap_err().into();
        assert!(matches!(error, ErrorKind::ParseNameError { .. }));
    }

    #[test]
    fn test_parse_timestamp() {
        assert_eq!(Metric::parse_timestamp(""), Ok(("", None)));
        assert_eq!(Metric::parse_timestamp("123"), Ok(("", Some(123))));
        assert_eq!(Metric::parse_timestamp(" -23"), Ok(("", Some(-23))));
    }

    #[test]
    fn test_parse_line() {
        let input = r#"
            # HELP http_requests_total The total number of HTTP requests.
            # TYPE http_requests_total counter
            http_requests_total{method="post",code="200"} 1027 1395066363000
            http_requests_total{method="post",code="400"}    3 1395066363000

            # Escaping in label values:
            msdos_file_access_time_seconds{path="C:\\DIR\\FILE.TXT",error="Cannot find file:\n\"FILE.TXT\""} 1.458255915e9

            # Minimalistic line:
            metric_without_timestamp_and_labels 12.47

            # A weird metric from before the epoch:
            something_weird{problem="division by zero"} +Inf -3982045

            # A histogram, which has a pretty complex representation in the text format:
            # HELP http_request_duration_seconds A histogram of the request duration.
            # TYPE http_request_duration_seconds histogram
            http_request_duration_seconds_bucket{le="0.05"} 24054
            http_request_duration_seconds_bucket{le="0.1"} 33444
            http_request_duration_seconds_bucket{le="0.2"} 100392
            http_request_duration_seconds_bucket{le="0.5"} 129389
            http_request_duration_seconds_bucket{le="1"} 133988
            http_request_duration_seconds_bucket{le="+Inf"} 144320
            http_request_duration_seconds_sum 53423
            http_request_duration_seconds_count 144320

            # Finally a summary, which has a complex representation, too:
            # HELP rpc_duration_seconds A summary of the RPC duration in seconds.
            # TYPE rpc_duration_seconds summary
            rpc_duration_seconds{quantile="0.01"} 3102
            rpc_duration_seconds{quantile="0.05"} 3272
            rpc_duration_seconds{quantile="0.5"} 4773
            rpc_duration_seconds{quantile="0.9"} 9001
            rpc_duration_seconds{quantile="0.99"} 76656
            rpc_duration_seconds_sum 1.7560473e+07
            rpc_duration_seconds_count 2693
            "#;
        assert!(input.lines().map(Line::parse).all(|r| r.is_ok()));
    }
}
