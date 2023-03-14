use std::{fmt, string::ToString};

use chrono::SecondsFormat;

use crate::Value;

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bytes(val) => write!(
                f,
                r#""{}""#,
                String::from_utf8_lossy(val)
                    .replace('\\', r#"\\"#)
                    .replace('"', r#"\""#)
                    .replace('\n', r#"\n"#)
            ),
            Self::Integer(val) => write!(f, "{val}"),
            Self::Float(val) => write!(f, "{val}"),
            Self::Boolean(val) => write!(f, "{val}"),
            Self::Object(map) => {
                let joined = map
                    .iter()
                    .map(|(key, val)| format!(r#""{key}": {val}"#))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "{{ {joined} }}")
            }
            Self::Array(array) => {
                let joined = array
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "[{joined}]")
            }
            Self::Timestamp(val) => {
                write!(f, "t'{}'", val.to_rfc3339_opts(SecondsFormat::AutoSi, true))
            }
            Self::Regex(regex) => write!(f, "r'{}'", **regex),
            Self::Null => write!(f, "null"),
        }
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use chrono::DateTime;
    use indoc::indoc;
    use ordered_float::NotNan;
    use regex::Regex;

    use super::Value;

    #[test]
    fn test_display_string() {
        assert_eq!(
            Value::Bytes(Bytes::from("Hello, world!")).to_string(),
            r#""Hello, world!""#
        );
    }

    #[test]
    fn test_display_string_with_backslashes() {
        assert_eq!(
            Value::Bytes(Bytes::from(r#"foo \ bar \ baz"#)).to_string(),
            r#""foo \\ bar \\ baz""#
        );
    }

    #[test]
    fn test_display_string_with_quotes() {
        assert_eq!(
            Value::Bytes(Bytes::from(r#""Hello, world!""#)).to_string(),
            r#""\"Hello, world!\"""#
        );
    }

    #[test]
    fn test_display_string_with_newlines() {
        assert_eq!(
            Value::Bytes(Bytes::from(indoc! {"
                Some
                new
                lines
            "}))
            .to_string(),
            r#""Some\nnew\nlines\n""#
        );
    }

    #[test]
    fn test_display_integer() {
        assert_eq!(Value::Integer(123).to_string(), "123");
    }

    #[test]
    fn test_display_float() {
        assert_eq!(
            Value::Float(NotNan::new(123.45).unwrap()).to_string(),
            "123.45"
        );
    }

    #[test]
    fn test_display_boolean() {
        assert_eq!(Value::Boolean(true).to_string(), "true");
    }

    #[test]
    fn test_display_object() {
        assert_eq!(
            Value::Object([("foo".into(), "bar".into())].into()).to_string(),
            r#"{ "foo": "bar" }"#
        );
    }

    #[test]
    fn test_display_array() {
        assert_eq!(
            Value::Array(
                vec!["foo", "bar"]
                    .into_iter()
                    .map(std::convert::Into::into)
                    .collect()
            )
            .to_string(),
            r#"["foo", "bar"]"#
        );
    }

    #[test]
    fn test_display_timestamp() {
        assert_eq!(
            Value::Timestamp(
                DateTime::parse_from_rfc3339("2000-10-10T20:55:36Z")
                    .unwrap()
                    .into()
            )
            .to_string(),
            "t'2000-10-10T20:55:36Z'"
        );
    }

    #[test]
    fn test_display_regex() {
        assert_eq!(
            Value::Regex(Regex::new(".*").unwrap().into()).to_string(),
            "r'.*'"
        );
    }

    #[test]
    fn test_display_null() {
        assert_eq!(Value::Null.to_string(), "null");
    }
}
