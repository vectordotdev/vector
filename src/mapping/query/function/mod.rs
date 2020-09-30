#![macro_use]

mod not;
pub(in crate::mapping) use not::NotFn;

use super::Function;
use crate::Event;
use crate::{event::Value, mapping::Result};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::str::FromStr;

/// Commonly used types when building new functions.
mod prelude {
    #[cfg(test)]
    pub(super) use super::RemapRegex;
    pub(super) use super::{is_scalar_value, ArgumentKind, ArgumentList, Parameter};
    pub(super) use crate::event::{Event, Value};
    pub(super) use crate::mapping::query::Function;
    #[cfg(test)]
    pub(super) use crate::mapping::query::Literal;
    pub(super) use crate::mapping::Result;
    pub(super) use crate::types::Conversion;
    pub(super) use std::convert::TryFrom;
}

// If this macro triggers, it means the logic to detect invalid types did not
// function as expected. This is a bug in the implementation.
macro_rules! unexpected_type {
    ($value:expr) => {
        unreachable!("unexpected value type: '{}'", $value.kind());
    };
}

macro_rules! required {
    ($ctx:expr, $fn:expr, $($pattern:pat => $then:expr),+ $(,)?) => {
        match $fn.execute($ctx)? {
            $($pattern => $then,)+
            v => unexpected_type!(v),
        }
    }
}

macro_rules! optional {
    ($ctx:expr, $fn:expr, $($pattern:pat => $then:expr),+ $(,)?) => {
        $fn.as_ref()
            .map(|v| v.execute($ctx))
            .transpose()?
            .map(|v| match v {
                $($pattern => $then,)+
                v => unexpected_type!(v),
            })
    }
}

macro_rules! build_signatures {
    ($($name:ident => $func:ident),* $(,)?) => {
        $(mod $name;)*

        $(pub(in crate::mapping) use self::$name::$func;)*

        #[derive(Debug, Copy, Clone, Eq, PartialEq)]
        #[allow(clippy::enum_variant_names)]
        pub(in crate::mapping) enum FunctionSignature {
            $($func,)*
        }

        impl FromStr for FunctionSignature {
            type Err = String;

            fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
                let func = match s {
                    $(stringify!($name) => Self::$func,)*
                    _ => return Err(format!("unknown function '{}'", s)),
                };

                Ok(func)
            }
        }

        impl FunctionSignature {
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$func => stringify!($name),)*
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

// List of built-in functions.
build_signatures! {
    to_string => ToStringFn,
    to_int => ToIntegerFn,
    to_float => ToFloatFn,
    to_bool => ToBooleanFn,
    to_timestamp => ToTimestampFn,
    parse_timestamp => ParseTimestampFn,
    strip_whitespace => StripWhitespaceFn,
    upcase => UpcaseFn,
    downcase => DowncaseFn,
    uuid_v4 => UuidV4Fn,
    md5 => Md5Fn,
    sha1 => Sha1Fn,
    now => NowFn,
    truncate => TruncateFn,
    parse_json => ParseJsonFn,
    format_timestamp => FormatTimestampFn,
    contains => ContainsFn,
    slice => SliceFn,
    tokenize => TokenizeFn,
    split => SplitFn,
}

pub(in crate::mapping) struct TypedArgument {
    pub resolver: Box<dyn Function>,
    pub parameter: Parameter,
}

impl TypedArgument {
    pub fn new(resolver: Box<dyn Function>, parameter: Parameter) -> Self {
        Self {
            resolver,
            parameter,
        }
    }
}

// delegates to resolver to satisfy tests in `mapping::parser`.
impl std::fmt::Debug for TypedArgument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.resolver.fmt(f)
    }
}

impl Function for TypedArgument {
    fn execute(&self, ctx: &Event) -> Result<Value> {
        let value = self.resolver.execute(ctx)?;

        // Ask the parameter if it accepts the given value.
        if !(self.parameter.accepts)(&value) {
            return Err(format!(
                "invalid argument type '{}' for parameter '{}'",
                value.kind(),
                self.parameter.keyword
            ));
        }

        Ok(value)
    }
}

/// Because the Regex object doesn't contain a global flag
/// (this is determined by calling either replace or replace_all)
/// we need to wrap the object and store this flag when we parse it.
#[derive(Debug, Clone)]
pub(in crate::mapping) struct RemapRegex {
    pub regex: regex::Regex,
    pub global: bool,
}

#[derive(Debug)]
pub(in crate::mapping) enum ArgumentKind {
    Value(Box<dyn Function>),
    Regex(RemapRegex),
}

impl ArgumentKind {
    pub fn into_value(self) -> Result<Box<dyn Function>> {
        if let ArgumentKind::Value(value) = self {
            Ok(value)
        } else {
            Err("expected value".into())
        }
    }
}

/// A parameter definition accepted by a function.
#[derive(Clone)]
pub(in crate::mapping) struct Parameter {
    /// The keyword of the parameter.
    ///
    /// Arguments can be passed in both using the keyword, or as a positional
    /// argument.
    pub keyword: &'static str,

    /// The parser calls this method to determine if a given argument value is
    /// accepted by the parameter.
    pub accepts: fn(&Value) -> bool,

    /// Whether or not this is a required parameter.
    ///
    /// If it isn't, the function can be called without errors, even if the
    /// argument matching this parameter is missing.
    pub required: bool,
}

impl std::fmt::Debug for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Parameter")
            .field("keyword", &self.keyword)
            .field("required", &self.required)
            .field("accepts", &"fn(&Value) -> bool".to_owned())
            .finish()
    }
}

#[derive(Debug, Default)]
pub(in crate::mapping) struct ArgumentList {
    /// The list of arguments provided to a function.
    arguments: Vec<Argument>,

    /// An optional mapping from argument keyword to position, if a keyword was
    /// provided for the given argument.
    keywords: HashMap<String, usize>,
}

impl ArgumentList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, argument: Argument, keyword: Option<String>) {
        self.arguments.push(argument);

        if let Some(keyword) = keyword {
            self.keywords.insert(keyword, self.arguments.len() - 1);
        }
    }

    pub fn optional(&mut self, keyword: &str) -> Option<ArgumentKind> {
        self.take(keyword).map(|arg| arg.kind)
    }

    pub fn required(&mut self, keyword: &str) -> Result<ArgumentKind> {
        self.take(keyword)
            .map(|arg| arg.kind)
            .ok_or(format!("unknown keyword: {}", keyword))
    }

    pub fn keywords(&self) -> Vec<&str> {
        self.keywords.keys().map(String::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    fn take(&mut self, keyword: &str) -> Option<Box<Argument>> {
        self.arguments
            .iter()
            .position(|a| a.parameter.keyword == keyword)
            .map(|i| self.arguments.remove(i))
            .map(|v| Box::new(v) as _)
    }
}

pub(in crate::mapping) struct Argument {
    kind: ArgumentKind,
    parameter: Parameter,
}

// delegates to resolver to satisfy tests in `mapping::parser`.
impl std::fmt::Debug for Argument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ArgumentKind::Value(value) => value.fmt(f),
            ArgumentKind::Regex(regex) => regex.fmt(f),
        }
    }
}

impl Argument {
    pub fn new(kind: ArgumentKind, parameter: Parameter) -> Self {
        Self { kind, parameter }
    }
}

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
