#[cfg(test)]
use super::Literal;
use super::{ArgumentList, Function, Parameter};
use crate::{
    event::{Event, Value},
    mapping::Result,
    types::Conversion,
};
use bytes::Bytes;
use chrono::{TimeZone, Utc};
use std::convert::TryFrom;
use std::str::FromStr;

// If this macro triggers, it means the logic to detect invalid types did not
// function as expected. This is a bug in the implementation.
macro_rules! unexpected_type {
    ($value:expr) => {
        unreachable!("unexpected value type: '{}'", $value.kind());
    };
}

macro_rules! build_signatures {
    ($($name:expr => $func:ident),* $(,)?) => {
        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        #[allow(clippy::enum_variant_names)]
        pub(in crate::mapping) enum FunctionSignature {
            $($func,)*
        }

        impl FromStr for FunctionSignature {
            type Err = String;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                let func = match s {
                    $($name => Self::$func,)*
                    _ => return Err(format!("unknown function '{}'", s)),
                };

                Ok(func)
            }
        }

        impl FunctionSignature {
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$func => $name,)*
                }
            }

            pub fn parameters(&self) -> &[Parameter] {
                match self {
                    $(Self::$func => $func::parameters(),)*
                }
            }

            pub fn into_boxed_function(self, arguments: ArgumentList) -> Result<Box<dyn Function>> {
                match self {
                    $(Self::$func => $func::try_from(arguments)
                        .map(|func| Box::new(func) as Box<dyn Function>),)*
                }
            }
        }
    };
}

build_signatures! {
    "to_string" => ToStringFn,
    "to_int" => ToIntegerFn,
    "to_float" => ToFloatFn,
    "to_bool" => ToBooleanFn,
    "to_timestamp" => ToTimestampFn,
    "parse_timestamp" => ParseTimestampFn,
    "strip_whitespace" => StripWhitespaceFn,
    "upcase" => UpcaseFn,
    "downcase" => DowncaseFn,
    "uuid_v4" => UuidV4Fn,
    "md5" => Md5Fn,
    "sha1" => Sha1Fn,
    "now" => NowFn,
    "truncate" => TruncateFn,
    "parse_json" => ParseJsonFn,
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Noop;

impl Function for Noop {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(Value::Null)
    }
}

//------------------------------------------------------------------------------

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
    default: Option<Box<dyn Function>>,
}

impl ToStringFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
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
                Some(v) => v.execute(ctx),
                None => Err(err),
            },
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |_| true,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |_| true,
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToStringFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToIntegerFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToIntegerFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
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
                _ => unexpected_type!(v),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => v.execute(ctx),
            None => Err(err),
        })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: is_scalar_value,
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToIntegerFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToFloatFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToFloatFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
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
                _ => unexpected_type!(v),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => v.execute(ctx),
            None => Err(err),
        })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: is_scalar_value,
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: is_scalar_value,
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToFloatFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToBooleanFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToBooleanFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
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
                _ => unexpected_type!(v),
            },
            Err(err) => Err(err),
        }
        .or_else(|err| match &self.default {
            Some(v) => v.execute(ctx),
            None => Err(err),
        })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_) | Value::Bytes(_) | Value::Boolean(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_) | Value::Bytes(_) | Value::Boolean(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToBooleanFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ToTimestampFn {
    query: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ToTimestampFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>, default: Option<Value>) -> Self {
        let default = default.map(|v| Box::new(Literal::from(v)) as _);
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
                    .ok_or(err)
                    .and_then(|v| v.execute(ctx))
                    .and_then(to_timestamp)
            })
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Bytes(_) | Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Bytes(_) | Value::Timestamp(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ToTimestampFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let default = arguments.optional("default");

        Ok(Self { query, default })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ParseTimestampFn {
    query: Box<dyn Function>,
    format: Box<dyn Function>,
    default: Option<Box<dyn Function>>,
}

impl ParseTimestampFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        format: &str,
        query: Box<dyn Function>,
        default: Option<Value>,
    ) -> Self {
        let format = Box::new(Literal::from(Value::from(format)));
        let default = default.map(|v| Box::new(Literal::from(v)) as _);

        Self {
            query,
            format,
            default,
        }
    }
}

impl Function for ParseTimestampFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let format = match self.format.execute(ctx)? {
            Value::Bytes(b) => format!("timestamp|{}", String::from_utf8_lossy(&b)),
            v => unexpected_type!(v),
        };

        let conversion: Conversion = format.parse().map_err(|e| format!("{}", e))?;

        let result = match self.query.execute(ctx) {
            Ok(v) => match v {
                Value::Bytes(_) => conversion.convert(v).map_err(|e| e.to_string()),
                Value::Timestamp(_) => Ok(v),
                _ => unexpected_type!(v),
            },
            Err(err) => Err(err),
        };
        if result.is_err() {
            if let Some(v) = &self.default {
                return match v.execute(ctx)? {
                    Value::Bytes(v) => conversion
                        .convert(Value::Bytes(v))
                        .map_err(|e| e.to_string()),
                    Value::Timestamp(v) => Ok(Value::Timestamp(v)),
                    v => unexpected_type!(v),
                };
            }
        }
        result
    }

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Timestamp(_)),
                required: true,
            },
            Parameter {
                keyword: "format",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "default",
                accepts: |v| matches!(v, Value::Bytes(_) | Value::Timestamp(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for ParseTimestampFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let format = arguments.required("format")?;
        let default = arguments.optional("default");

        Ok(Self {
            query,
            format,
            default,
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct StripWhitespaceFn {
    query: Box<dyn Function>,
}

impl StripWhitespaceFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        Self { query }
    }
}

impl Function for StripWhitespaceFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx)? {
            Value::Bytes(b) => std::str::from_utf8(&b)
                .map(|s| Value::Bytes(b.slice_ref(s.trim().as_bytes())))
                .map_err(|_| {
                    "unable to strip white_space from non-unicode string types".to_owned()
                }),
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for StripWhitespaceFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct UpcaseFn {
    query: Box<dyn Function>,
}

impl UpcaseFn {
    #[cfg(test)]
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
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for UpcaseFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct DowncaseFn {
    query: Box<dyn Function>,
}

impl DowncaseFn {
    #[cfg(test)]
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
            value => unexpected_type!(value),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for DowncaseFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct UuidV4Fn {}

impl UuidV4Fn {
    #[cfg(test)]
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

impl TryFrom<ArgumentList> for UuidV4Fn {
    type Error = String;

    fn try_from(_: ArgumentList) -> Result<Self> {
        Ok(Self {})
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Sha1Fn {
    query: Box<dyn Function>,
}

impl Sha1Fn {
    #[cfg(test)]
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
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for Sha1Fn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct Md5Fn {
    query: Box<dyn Function>,
}

impl Md5Fn {
    #[cfg(test)]
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
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for Md5Fn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct NowFn {}

impl NowFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new() -> Self {
        Self {}
    }
}

impl Function for NowFn {
    fn execute(&self, _: &Event) -> Result<Value> {
        Ok(Value::Timestamp(Utc::now()))
    }
}

impl TryFrom<ArgumentList> for NowFn {
    type Error = String;

    fn try_from(_: ArgumentList) -> Result<Self> {
        Ok(Self {})
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct TruncateFn {
    query: Box<dyn Function>,
    limit: Box<dyn Function>,
    ellipsis: Option<Box<dyn Function>>,
}

impl TruncateFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(
        query: Box<dyn Function>,
        limit: Box<dyn Function>,
        ellipsis: Option<Value>,
    ) -> Self {
        let ellipsis = ellipsis.map(|b| Box::new(Literal::from(b)) as _);

        Self {
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

            let ellipsis = match &self.ellipsis {
                None => false,
                Some(v) => match v.execute(ctx)? {
                    Value::Boolean(value) => value,
                    v => unexpected_type!(v),
                },
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

    fn parameters() -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "value",
                accepts: |v| matches!(v, Value::Bytes(_)),
                required: true,
            },
            Parameter {
                keyword: "limit",
                accepts: |v| matches!(v, Value::Integer(_) | Value::Float(_)),
                required: true,
            },
            Parameter {
                keyword: "ellipsis",
                accepts: |v| matches!(v, Value::Boolean(_)),
                required: false,
            },
        ]
    }
}

impl TryFrom<ArgumentList> for TruncateFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;
        let limit = arguments.required("limit")?;
        let ellipsis = arguments.optional("ellipsis");

        Ok(Self {
            query,
            limit,
            ellipsis,
        })
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(in crate::mapping) struct ParseJsonFn {
    query: Box<dyn Function>,
}

impl ParseJsonFn {
    #[cfg(test)]
    pub(in crate::mapping) fn new(query: Box<dyn Function>) -> Self {
        ParseJsonFn { query }
    }
}

impl Function for ParseJsonFn {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        match self.query.execute(ctx)? {
            Value::Bytes(b) => serde_json::from_slice(&b)
                .map(|v: serde_json::Value| v.into())
                .map_err(|err| format!("unable to parse json {}", err)),
            v => unexpected_type!(v),
        }
    }

    fn parameters() -> &'static [Parameter] {
        &[Parameter {
            keyword: "value",
            accepts: |v| matches!(v, Value::Bytes(_)),
            required: true,
        }]
    }
}

impl TryFrom<ArgumentList> for ParseJsonFn {
    type Error = String;

    fn try_from(mut arguments: ArgumentList) -> Result<Self> {
        let query = arguments.required("value")?;

        Ok(Self { query })
    }
}

//------------------------------------------------------------------------------

fn is_scalar_value(value: &Value) -> bool {
    match value {
        Value::Integer(_)
        | Value::Float(_)
        | Value::Bytes(_)
        | Value::Boolean(_)
        | Value::Timestamp(_) => true,
        Value::Map(_) | Value::Array(_) | Value::Null => false,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::query::path::Path;
    use chrono::DateTime;
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
                    Some(Value::from("1970-01-01T00:00:10Z")),
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
                ),
            ),
            (
                Event::from(""),
                Ok(Value::Timestamp(
                    DateTime::parse_from_str(
                        "1983 Apr 13 12:09:14.274 +0000",
                        "%Y %b %d %H:%M:%S%.3f %z",
                    )
                    .unwrap()
                    .with_timezone(&Utc),
                )),
                ParseTimestampFn::new(
                    "%Y %b %d %H:%M:%S%.3f %z",
                    Box::new(Path::from(vec![vec!["foo"]])),
                    Some(Value::from("1983 Apr 13 12:09:14.274 +0000")),
                ),
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
                ),
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
                ),
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
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'integer'")]
    fn check_upcase_invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Integer(20));

        let _ = UpcaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
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
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'integer'")]
    fn check_downcase_invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Integer(20));

        let _ = DowncaseFn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
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
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'boolean'")]
    fn check_sha1_invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Boolean(true));

        let _ = Sha1Fn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
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
        ];

        for (input_event, exp, query) in cases {
            assert_eq!(query.execute(&input_event), exp);
        }
    }

    #[test]
    #[should_panic(expected = "unexpected value type: 'boolean'")]
    fn check_md5_invalid_type() {
        let mut event = Event::from("");
        event.as_mut_log().insert("foo", Value::Boolean(true));

        let _ = Md5Fn::new(Box::new(Path::from(vec![vec!["foo"]]))).execute(&event);
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
                    None,
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
