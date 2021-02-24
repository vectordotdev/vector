use nom::{
    branch::alt,
    bytes::complete::{escaped, is_not, tag},
    character::complete::{one_of, space0},
    combinator::{all_consuming, map, opt, rest, verify},
    error::ErrorKind,
    multi::many0,
    sequence::{delimited, terminated},
};

pub fn parse(input: &str) -> Vec<&str> {
    let simple = is_not::<_, _, (&str, ErrorKind)>(" \t[\"");
    let string = delimited(
        tag("\""),
        map(opt(escaped(is_not("\"\\"), '\\', one_of("\"\\"))), |o| {
            o.unwrap_or("")
        }),
        tag("\""),
    );
    let bracket = delimited(
        tag("["),
        map(opt(escaped(is_not("]\\"), '\\', one_of("]\\"))), |o| {
            o.unwrap_or("")
        }),
        tag("]"),
    );

    // fall back to returning the rest of the input, if any
    let remainder = verify(rest, |s: &str| !s.is_empty());
    let field = alt((bracket, string, simple, remainder));

    all_consuming(many0(terminated(field, space0)))(input)
        .expect("parser should always succeed")
        .1
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn basic() {
        assert_eq!(parse("foo"), &["foo"]);
    }

    #[test]
    fn multiple() {
        assert_eq!(parse("foo bar"), &["foo", "bar"]);
    }

    #[test]
    fn more_space() {
        assert_eq!(parse("foo\t bar"), &["foo", "bar"]);
    }

    #[test]
    fn so_much_space() {
        assert_eq!(parse("foo  \t bar     baz"), &["foo", "bar", "baz"]);
    }

    #[test]
    fn quotes() {
        assert_eq!(parse(r#"foo "bar baz""#), &["foo", r#"bar baz"#]);
    }

    #[test]
    fn empty_quotes() {
        assert_eq!(parse(r#"foo """#), &["foo", ""]);
    }

    #[test]
    fn escaped_quotes() {
        assert_eq!(
            parse(r#"foo "bar \" \" baz""#),
            &["foo", r#"bar \" \" baz"#],
        );
    }

    #[test]
    fn unclosed_quotes() {
        assert_eq!(parse(r#"foo "bar"#), &["foo", "\"bar"],);
    }

    #[test]
    fn brackets() {
        assert_eq!(parse("[foo.bar = baz] quux"), &["foo.bar = baz", "quux"],);
    }

    #[test]
    fn empty_brackets() {
        assert_eq!(parse("[] quux"), &["", "quux"],);
    }

    #[test]
    fn escaped_brackets() {
        assert_eq!(
            parse(r#"[foo " [[ \] "" bar] baz"#),
            &[r#"foo " [[ \] "" bar"#, "baz"],
        );
    }

    #[test]
    fn unclosed_brackets() {
        assert_eq!(parse("foo [bar"), &["foo", "[bar"],);
    }

    #[test]
    fn truncated_field() {
        assert_eq!(
            parse("foo bar[baz]: quux"),
            &["foo", "bar", "baz", ":", "quux"]
        );
        assert_eq!(parse("foo bar[baz quux"), &["foo", "bar", "[baz quux"]);
    }

    #[test]
    fn dash_field() {
        assert_eq!(parse("foo - bar"), &["foo", "-", "bar"]);
    }

    #[test]
    fn from_fuzzing() {
        assert_eq!(parse("").len(), 0);
        assert_eq!(parse("f] bar"), &["f]", "bar"]);
        assert_eq!(parse("f\" bar"), &["f", "\" bar"]);
        assert_eq!(parse("f[f bar"), &["f", "[f bar"]);
        assert_eq!(parse("f\"f bar"), &["f", "\"f bar"]);
        assert_eq!(parse("[][x"), &["", "[x"]);
        assert_eq!(parse("x[][x"), &["x", "", "[x"]);
    }
}
