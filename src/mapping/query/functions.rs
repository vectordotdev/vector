use super::Function;
use crate::{
    event::{Event, Value},
    mapping::Result,
    types::Conversion,
};
use bytes::{Bytes, BytesMut};

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
        let value = self.query.execute(ctx)?;

        if let Value::Bytes(bytes) = value {
            let mut buf = BytesMut::with_capacity(bytes.len());

            buf.extend_from_slice(&bytes);
            buf.iter_mut().for_each(|c| c.make_ascii_uppercase());

            return Ok(Value::Bytes(buf.freeze()));
        }

        Ok(value)
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
        let value = self.query.execute(ctx)?;

        if let Value::Bytes(bytes) = value {
            let mut buf = BytesMut::with_capacity(bytes.len());

            buf.extend_from_slice(&bytes);
            buf.iter_mut().for_each(|c| c.make_ascii_lowercase());

            return Ok(Value::Bytes(buf.freeze()));
        }

        Ok(value)
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
        let value = self.query.execute(ctx)?;

        if let Value::Bytes(bytes) = value {
            let sha = sha1::Sha1::from(bytes).hexdigest();
            return Ok(Value::Bytes(sha.into()));
        }

        Ok(value)
    }
}

//------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::{path::Path, Literal};
    use chrono::{DateTime, Utc};

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
                Ok(Value::Integer(20)),
                UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Ok(Value::Boolean(true)),
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
                Ok(Value::Integer(20)),
                DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Ok(Value::Boolean(true)),
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
            Value::Bytes(value) => uuid::Uuid::from_slice(&value).expect("valid UUID V4"),
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
                Ok(Value::Integer(20)),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
            (
                {
                    let mut event = Event::from("");
                    event.as_mut_log().insert("foo", Value::Boolean(true));
                    event
                },
                Ok(Value::Boolean(true)),
                Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))),
            ),
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }
}
