use super::Function;
use crate::{
    event::{Event, Value},
    mapping::Result,
    types::Conversion,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};

#[derive(Debug)]
pub(in crate::mapping) struct NotFn {
    query: Box<dyn Function>,
}

impl NotFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for NotFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        self.query.execute(ctx).and_then(|v| match v {
            Value::Boolean(b) => Ok(Value::Boolean(!b)),
            _ => Err(format!("unable to perform NOT on {:?} value", v)),
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToStringFn {
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ToStringFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        Self { query, default }
    }
}

impl Function for ToStringFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx) {
            Ok(v) => Ok(match v {
                Value::Bytes(_) => v,
                _ => Value::Bytes(v.as_bytes()),
            }),
            Err(err) => match &self.default {
                Some(v) => Ok(v.clone()),
                None => Err(err),
            },
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToIntegerFn {
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ToIntegerFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        Self { query, default }
    }
}

impl Function for ToIntegerFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Integer(_) => Ok(v),
                Value::Float(f) => Ok(Value::Integer(f as i64)),
                Value::Bytes(_) => Conversion::Integer.convert(v).map_err(|e| e.to_string()),
                Value::Boolean(b) => Ok(Value::Integer(if b { 1 } else { 0 })),
                Value::Timestamp(t) => Ok(Value::Integer(t.timestamp())),
                Value::Null => Err("value is null".to_string()),
                _ => Err("unable to convert array or object into int".to_string()),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => Ok(v.clone()),
            None => Err(err),
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToFloatFn {
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ToFloatFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        Self { query, default }
    }
}

impl Function for ToFloatFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Float(_) => Ok(v),
                Value::Integer(i) => Ok(Value::Float(i as f64)),
                Value::Bytes(_) => Conversion::Float.convert(v).map_err(|e| e.to_string()),
                Value::Boolean(b) => Ok(Value::Float(if b { 1.0 } else { 0.0 })),
                Value::Timestamp(t) => Ok(Value::Float(t.timestamp() as f64)),
                Value::Null => Err("value is null".to_string()),
                _ => Err("unable to convert array or object into float".to_string()),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => Ok(v.clone()),
            None => Err(err),
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToBooleanFn {
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ToBooleanFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        Self { query, default }
    }
}

impl Function for ToBooleanFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Boolean(_) => Ok(v),
                Value::Float(f) => Ok(Value::Boolean(f != 0.0)),
                Value::Integer(i) => Ok(Value::Boolean(i != 0)),
                Value::Bytes(_) => Conversion::Boolean.convert(v).map_err(|e| e.to_string()),
                Value::Timestamp(_) => Err("unable to convert timestamp into bool".to_string()),
                Value::Null => Err("value is null".to_string()),
                _ => Err("unable to convert array or object into bool".to_string()),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => Ok(v.clone()),
            None => Err(err),
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToTimestampFn {
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ToTimestampFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        Self { query, default }
    }
}

impl Function for ToTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        self.query
            .execute(ctx)
            .and_then(to_timestamp)
            .or_else(|err| {
                self.default
                    .as_ref()
                    .cloned()
                    .ok_or(err)
                    .and_then(to_timestamp)
            })
    }
}

fn to_timestamp(value: Value) -> Result<Value> {
    match value {
        Value::Bytes(_) => Conversion::Timestamp
            .convert(value)
            .map_err(|e| e.to_string()),
        Value::Integer(i) => Ok(Value::Timestamp(Utc.timestamp(i, 0))),
        Value::Timestamp(_) => Ok(value),
        _ => Err("unable to parse non-string or integer type to timestamp".to_string()),
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ParseTimestampFn {
    conversion: Conversion,
    query: Box<dyn Function>,
    default: Option<Value>,
}

impl ParseTimestampFn {
    pub(in crate::mapping) fn new(
        format: &str,
        query: Box<dyn Function>,
        default: Option<Value>,
    ) -> Result<Self> {
        let conversion: Conversion = ("timestamp|".to_string() + format)
            .parse()
            .map_err(|e| format!("{}", e))?;
        Ok(Self {
            conversion,
            query,
            default,
        })
    }
}

impl Function for ParseTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let result = match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Bytes(_) => self.conversion.convert(v).map_err(|e| e.to_string()),
                Value::Timestamp(_) => Ok(v),
                Value::Boolean(_) => Err("unable to convert boolean into timestamp".to_string()),
                Value::Float(_) => Err("unable to convert float into timestamp".to_string()),
                Value::Integer(_) => Err("unable to convert integer into timestamp".to_string()),
                Value::Null => Err("value is null".to_string()),
                _ => Err("unable to convert array or object into timestamp".to_string()),
            },
            Err(err) => Err(err),
        };
        if result.is_err() {
            if let Some(v) = &self.default {
                return Ok(v.clone());
            }
        }
        result
    }
}

#[derive(Debug)]
pub(in crate::mapping) struct StripWhitespaceFn {
    query: Box<dyn Function>,
}

impl StripWhitespaceFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for StripWhitespaceFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = self.query.execute(ctx)?;
        if let Value::Bytes(bytes) = value {
            // Convert it to a str which will validate that it is valid utf8,
            // and will give us a trim function.
            // This does not need to allocate any additional memory.
            if let Ok(s) = std::str::from_utf8(&bytes) {
                Ok(Value::Bytes(bytes.slice_ref(s.trim().as_bytes())))
            } else {
                // Not a valid unicode string.
                Err("unable to strip white_space from non-unicode string types".to_string())
            }
        } else {
            Err("unable to strip white_space from non-string types".to_string())
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct UpcaseFn {
    query: Box<dyn Function>,
}

impl UpcaseFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for UpcaseFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => Ok(Value::Bytes(
                String::from_utf8_lossy(&bytes).to_uppercase().into(),
            )),
            _ => Err(r#"unable to apply "upcase" to non-string types"#.to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct DowncaseFn {
    query: Box<dyn Function>,
}

impl DowncaseFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for DowncaseFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => Ok(Value::Bytes(
                String::from_utf8_lossy(&bytes).to_lowercase().into(),
            )),
            _ => Err(r#"unable to apply "downcase" to non-string types"#.to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct UuidV4Fn {}

impl UuidV4Fn {
    pub(in crate::mapping) fn new() -> Self {
        Self {}
    }
}

impl Function for UuidV4Fn {
    fn execute(&self, _: &Event) -> Result<Value> {
        let mut buf = [0; 36];
        let uuid = uuid::Uuid::new_v4().to_hyphenated().encode_lower(&mut buf);

        Ok(Value::Bytes(Bytes::copy_from_slice(uuid.as_bytes())))
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Sha1Fn {
    query: Box<dyn Function>,
}

impl Sha1Fn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for Sha1Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        use sha1::{Digest, Sha1};

        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => {
                let sha1 = hex::encode(Sha1::digest(&bytes));
                Ok(Value::Bytes(sha1.into()))
            }
            _ => Err(r#"unable to apply "sha1" to non-string types"#.to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Md5Fn {
    query: Box<dyn Function>,
}

impl Md5Fn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for Md5Fn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        use md5::{Digest, Md5};

        match self.query.execute(ctx)? {
            Value::Bytes(bytes) => {
                let md5 = hex::encode(Md5::digest(&bytes));
                Ok(Value::Bytes(md5.into()))
            }
            _ => Err(r#"unable to apply "md5" to non-string types"#.to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct NowFn {}

impl NowFn {
    pub(in crate::mapping) fn new() -> Self {
        Self {}
    }
}

impl Function for NowFn {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(Value::Timestamp(Utc::now()))
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct TruncateFn {
    query: Box<dyn Function>,
    limit: Box<dyn Function>,
    ellipsis: Option<Value>,
}

impl TruncateFn {
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        limit: Box<dyn Function>,
        ellipsis: Option<Value>,
    ) -> Self {
        TruncateFn {
            query,
            limit,
            ellipsis,
        }
    }
}

impl Function for TruncateFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = self.query.execute(ctx)?;
        if let Value::Bytes(bytes) = value {
            let limit = match self.limit.execute(ctx)? {
                // If the result of execution is a float, we take the floor as our limit.
                Value::Float(f) => f.floor() as usize,
                Value::Integer(i) if i >= 0 => i as usize,
                _ => return Err("limit is not a positive number".into()),
            };

            let ellipsis = match self.ellipsis {
                None => false,
                Some(Value::Boolean(value)) => value,
                _ => return Err("ellipsis is not a boolean".into()),
            };

            if let Ok(s) = std::str::from_utf8(&bytes) {
                let pos = if let Some((pos, chr)) = s.char_indices().take(limit).last() {
                    // char_indices gives us the starting position of the character at limit,
                    // we want the end position.
                    pos + chr.len_utf8()
                } else {
                    // We have an empty string
                    0
                };

                if s.len() <= pos {
                    // No truncating necessary.
                    Ok(Value::Bytes(bytes))
                } else if ellipsis {
                    // Allocate a new string to add the ellipsis to.
                    let mut new = s[0..pos].to_string();
                    new.push_str("...");
                    Ok(Value::Bytes(new.into()))
                } else {
                    // Just pull the relevant part out of the original parameter.
                    Ok(Value::Bytes(bytes.slice(0..pos)))
                }
            } else {
                // Not a valid utf8 string.
                Err("unable to truncate from non-unicode string types".to_string())
            }
        } else {
            Err("unable to truncate non-string types".to_string())
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ParseJsonFn {
    query: Box<dyn Function>,
}

impl ParseJsonFn {
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        ParseJsonFn { query }
    }
}

impl Function for ParseJsonFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = self.query.execute(ctx)?;
        if let Value::Bytes(bytes) = value {
            match serde_json::from_slice::<serde_json::Value>(&bytes) {
                Ok(value) => Ok(value.into()),
                Err(err) => Err(format!("unable to parse json {}", err)),
            }
        } else {
            Err("unable to apply \"parse_json\" to non-string types".to_string())
        }
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::{path::Path, Literal};
    use chrono::{DateTime, Utc};
    use std::collections::BTreeMap;

    #[test]
    fn check_not_operator() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                NotFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(false)),
                NotFn::new(Box::new(Literal::from(Value::Boolean(true)))),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                NotFn::new(Box::new(Literal::from(Value::Boolean(false)))),
            ),
            (
                Event::from(""),
                Err("unable to perform NOT on Bytes(b\"not a bool\") value".to_string()),
                NotFn::new(Box::new(Literal::from(Value::from("not a bool")))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_string_conversions() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::from("default")),
                ToStringFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("default")),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Ok(Value::from("20")),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Float(20.5));
                    event
                },
                Ok(Value::from("20.5")),
                ToStringFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_int_conversions() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Integer(10)),
                ToIntegerFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Integer(10)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("20"));
                    event
                },
                Ok(Value::Integer(20)),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Float(20.5));
                    event
                },
                Ok(Value::Integer(20)),
                ToIntegerFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_float_conversions() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToFloatFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Float(10.0)),
                ToFloatFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Float(10.0)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("20.5"));
                    event
                },
                Ok(Value::Float(20.5)),
                ToFloatFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Ok(Value::Float(20.0)),
                ToFloatFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_bool_conversions() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("true"));
                    event
                },
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Ok(Value::Boolean(true)),
                ToBooleanFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_timestamp_conversions() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ToTimestampFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-01-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Integer(10)),
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-01-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::Bytes("10".into())),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::Timestamp(
                            DateTime::parse_from_rfc3339("1970-02-01T00:00:10Z")
                                .unwrap()
                                .with_timezone(&Utc),
                        ),
                    );
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc3339("1970-02-01T00:00:10Z")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ToTimestampFn::new(Box::new(Path::from(vec![vec!["foo"]])), None),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_timestamp_parsing() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                ParseTimestampFn::new(
                    "%a %b %e %T %Y",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                )
                .unwrap(),
            ),
            (
                Event::from(""),
                Ok(Value::from("foobar")),
                ParseTimestampFn::new(
                    "%a %b %e %T %Y",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("foobar")),
                )
                .unwrap(),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::Timestamp(
                            DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                                .unwrap()
                                .with_timezone(&Utc),
                        ),
                    );
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ParseTimestampFn::new(
                    "%d/%m/%Y:%H:%M:%S %z",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                )
                .unwrap(),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("16/10/2019:12:00:00 +0000"));
                    event
                },
                Ok(Value::Timestamp(
                    DateTime::parse_from_rfc2822("Wed, 16 Oct 2019 12:00:00 +0000")
                        .unwrap()
                        .with_timezone(&Utc),
                )),
                ParseTimestampFn::new(
                    "%d/%m/%Y:%H:%M:%S %z",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    None,
                )
                .unwrap(),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_upcase() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo 2 bar"));
                    event
                },
                Ok(Value::from("FOO 2 BAR")),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Err(r#"unable to apply "upcase" to non-string types"#.to_string()),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Err(r#"unable to apply "upcase" to non-string types"#.to_string()),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_downcase() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("FOO 2 bar"));
                    event
                },
                Ok(Value::from("foo 2 bar")),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Err(r#"unable to apply "downcase" to non-string types"#.to_string()),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Err(r#"unable to apply "downcase" to non-string types"#.to_string()),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_uuid_v4() {
        match UuidV4Fn::new().execute(&Event::from("")).unwrap() {
            Value::Bytes(value) => {
                uuid::Uuid::parse_str(std::str::from_utf8(&value).unwrap()).expect("valid UUID V4")
            }
            _ => panic!("unexpected uuid_v4 output"),
        };
    }

    #[test]
    fn check_sha1() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from("0beec7b5ea3f0fdbc95d0dd47f3c5bc275da8a33")),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Err(r#"unable to apply "sha1" to non-string types"#.to_string()),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Err(r#"unable to apply "sha1" to non-string types"#.to_string()),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_md5() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("foo"));
                    event
                },
                Ok(Value::from("acbd18db4cc2f85cedef654fccc4a4d8")),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Integer(20));
                    event
                },
                Err(r#"unable to apply "md5" to non-string types"#.to_string()),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Err(r#"unable to apply "md5" to non-string types"#.to_string()),
                Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_strip_whitespace() {
        let cases = vec![
            (
                Event::from(""),
                Err("path .foo not found in event".to_string()),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from(""));
                    event
                },
                Ok(Value::Bytes("".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("    "));
                    event
                },
                Ok(Value::Bytes("".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("hi there"));
                    event
                },
                Ok(Value::Bytes("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("     hi there    "));
                    event
                },
                Ok(Value::Bytes("hi there".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert(
                        "foo",
                        Value::from(" \u{3000}\u{205F}\u{202F}\u{A0}\u{9} ❤❤ hi there ❤❤  \u{9}\u{A0}\u{202F}\u{205F}\u{3000} "),
                    );
                    event
                },
                Ok(Value::Bytes("❤❤ hi there ❤❤".into())),
                StripWhitespaceFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_truncate() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(0.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(0.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(10.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("Super"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("Supercalifragilisticexpialidocious"));
                    event
                },
                Ok(Value::Bytes("Super".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(false)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("♔♕♖♗♘♙♚♛♜♝♞♟"));
                    event
                },
                Ok(Value::Bytes("♔♕♖♗♘♙...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(6.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("Supercalifragilisticexpialidocious"));
                    event
                },
                Ok(Value::Bytes("Super...".into())),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Float(3.0));
                    event
                },
                Err("unable to truncate non-string types".to_string()),
                TruncateFn::new(
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Box::new(Literal::from(Value::Float(5.0))),
                    Some(Value::Boolean(true)),
                ),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    fn check_parse_json() {
        let cases = vec![
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("42"));
                    event
                },
                Ok(Value::from(42)),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::from("\"hello\""));
                    event
                },
                Ok(Value::from("hello")),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("{\"field\": \"value\"}"));
                    event
                },
                Ok(Value::Map({
                    let mut map = BTreeMap::new();
                    map.insert("field".into(), Value::from("value"));
                    map
                })),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event
                        .as_mut_log()
                        .insert("foo", Value::from("{\"field\"x \"value\"}"));
                    event
                },
                Err("unable to parse json expected `:` at line 1 column 9".into()),
                ParseJsonFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
