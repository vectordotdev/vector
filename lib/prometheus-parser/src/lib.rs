use nom::{
    branch::alt,
    bytes::complete::{is_not, tag, take_while, take_while1},
    character::complete::char,
    combinator::{map, opt, value},
    error::ParseError,
    multi::{fold_many0, separated_list},
    number::complete::double,
    sequence::{delimited, pair, preceded, tuple},
};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum MetricKind {
    Counter,
    Gauge,
    Histogram,
    Summary,
    Untyped,
}

/// Each line of Prometheus text format.
/// We discard empty lines, comments, and timestamps.
#[derive(Debug)]
pub enum MetricLine {
    Type {
        metric_name: String,
        kind: MetricKind,
    },
    Metric {
        name: String,
        labels: BTreeMap<String, String>,
        value: f64,
    },
}

fn trim_space(input: &str) -> &str {
    input.trim_start_matches(|c| c == ' ' || c == '\t')
}

fn sp<'a, E: ParseError<&'a str>>(i: &'a str) -> nom::IResult<&'a str, &'a str, E> {
    take_while(|c| c == ' ' || c == '\t')(i)
}

impl MetricLine {
    /// Name matches the regex `[a-zA-Z_][a-zA-Z0-9_]*`.
    fn parse_name(input: &str) -> nom::IResult<&str, String> {
        let input = trim_space(input);
        let (input, (a, b)) = pair(
            take_while1(|c: char| c.is_alphabetic() || c == '_'),
            take_while(|c: char| c.is_alphanumeric() || c == '_'),
        )(input)?;
        Ok((input, a.to_owned() + b))
    }

    /// Float value, and +Inf, -Int, Nan.
    fn parse_value(input: &str) -> nom::IResult<&str, f64> {
        let input = trim_space(input);
        alt((
            value(f64::INFINITY, tag("+Inf")),
            value(f64::NEG_INFINITY, tag("-Inf")),
            value(f64::NAN, tag("Nan")),
            double,
        ))(input)
    }

    /// `# TYPE <metric_name> <metric_type>`
    fn parse_type(input: &str) -> nom::IResult<&str, Self> {
        let input = trim_space(input);
        let (input, _) = tag("#")(input)?;
        let input = trim_space(input);
        let (input, _) = tag("TYPE")(input)?;
        let (input, metric_name) = Self::parse_name(input)?;
        let input = trim_space(input);
        let (input, kind) = alt((
            value(MetricKind::Counter, tag("counter")),
            value(MetricKind::Gauge, tag("gauge")),
            value(MetricKind::Summary, tag("summary")),
            value(MetricKind::Histogram, tag("histogram")),
            value(MetricKind::Untyped, tag("untyped")),
        ))(input)?;
        Ok((input, Self::Type { metric_name, kind }))
    }

    /// Parse `{label_name="value",...}`
    fn parse_labels(input: &str) -> nom::IResult<&str, BTreeMap<String, String>> {
        let input = trim_space(input);
        let parse_labels_inner = map(
            separated_list(
                preceded(sp, char(',')),
                tuple((
                    Self::parse_name,
                    preceded(sp, char('=')),
                    parse_escaped_string,
                )),
            ),
            |list| list.into_iter().map(|(k, _, v)| (k, v)).collect(),
        );
        map(
            // TODO: `opt` replace all errors with `None`,
            // but only error in `char('{')` should be accepted.
            opt(delimited(char('{'), parse_labels_inner, char('}'))),
            |r| r.unwrap_or_default(),
        )(input)
    }

    /// Parse a single line with format
    /// ```
    /// metric_name [
    ///   "{" label_name "=" `"` label_value `"` { "," label_name "=" `"` label_value `"` } [ "," ] "}"
    /// ] value [ timestamp ]
    /// ```
    ///
    /// We don't parse timestamp.
    fn parse_metric(input: &str) -> nom::IResult<&str, Self> {
        let input = trim_space(input);
        let (input, name) = Self::parse_name(input)?;
        let (input, labels) = Self::parse_labels(input)?;
        let (input, value) = Self::parse_value(input)?;
        Ok((
            input,
            Self::Metric {
                name,
                labels,
                value,
            },
        ))
    }

    pub fn parse(input: &str) -> nom::IResult<&str, Self> {
        alt((Self::parse_type, Self::parse_metric))(input)
    }
}

/// Parse `'"' string_content '"'`. `string_content` can contain any unicode characters,
/// backslash (`\`), double-quote (`"`), and line feed (`\n`) characters have to be
/// escaped as `\\`, `\"`, and `\n`, respectively.
fn parse_escaped_string(input: &str) -> nom::IResult<&str, String> {
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
        String::new(),
        |mut result, fragment| {
            match fragment {
                StringFragment::Literal(s) => result.push_str(s),
                StringFragment::EscapedChar(c) => result.push(c),
            }
            result
        },
    );

    delimited(char('"'), build_string, char('"'))(input)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_escaped_string() {
        fn wrap(s: &str) -> String {
            format!("  \t \"{}\"  .", s)
        }

        // parser should not consume more that it needed
        let tail = "  .";

        let input = wrap("");
        let (left, r) = parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "");

        let input = wrap(r#"a\\ asdf"#);
        let (left, r) = parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "a\\ asdf");

        let input = wrap(r#"\"\""#);
        let (left, r) = parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\"\"");

        let input = wrap(r#"\"\\\n"#);
        let (left, r) = parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\"\\\n");

        let input = wrap(r#"\\n"#);
        let (left, r) = parse_escaped_string(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "\\n");

        let input = wrap(r#"  😂  "#);
        let (left, r) = parse_escaped_string(&input).unwrap();
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
        let (left, r) = MetricLine::parse_name(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "abc_def");

        let input = wrap("__9A0bc_def__");
        let (left, r) = MetricLine::parse_name(&input).unwrap();
        assert_eq!(left, tail);
        assert_eq!(r, "__9A0bc_def__");

        let input = wrap("99");
        assert!(MetricLine::parse_name(&input).is_err());
    }
}
