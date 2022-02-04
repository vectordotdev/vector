#![macro_use]

mod not;

use std::{collections::HashMap, convert::TryFrom, str::FromStr};

pub(in crate::mapping) use not::NotFn;

use super::Function;
use crate::{
    event::Event,
    mapping::{query::query_value::QueryValue, Result},
};

/// Commonly used types when building new functions.
mod prelude {
    pub(super) use std::convert::TryFrom;

    pub(super) use super::{ArgumentList, Parameter};
    #[cfg(test)]
    pub(super) use crate::mapping::query::Literal;
    pub(super) use crate::{
        event::{Event, Value},
        mapping::{
            query::{query_value::QueryValue, Function},
            Result,
        },
    };
}

// If this macro triggers, it means the logic to detect invalid types did not
// function as expected. This is a bug in the implementation.
macro_rules! unexpected_type {
    ($value:expr) => {
        unreachable!("unexpected value type: '{}'", $value.kind())
    };
}

macro_rules! required {
    ($ctx:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        match $fn.execute($ctx)? {
            $($pattern $(if $if)? => $then,)+
            v => unexpected_type!(v),
        }
    }
}

macro_rules! required_value {
    ($ctx:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        required!($ctx, $fn,
            QueryValue::Value(value) => match value {
                $($pattern $(if $if)? => $then,)+
                v => unexpected_type!(v),
            })
    }
}

macro_rules! optional {
    ($ctx:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        $fn.as_ref()
            .map(|v| v.execute($ctx))
            .transpose()?
            .map(|v| match v {
                $($pattern $(if $if)? => $then,)+
                v => unexpected_type!(v),
            })
    }
}

macro_rules! optional_value {
    ($ctx:expr, $fn:expr, $($pattern:pat $(if $if:expr)? => $then:expr),+ $(,)?) => {
        optional!($ctx, $fn,
                  QueryValue::Value(value) => match value {
                      $($pattern $(if $if)? => $then,)+
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
    split => SplitFn,
    flatten => FlattenFn,
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
    pub accepts: fn(&QueryValue) -> bool,

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

    pub fn optional(&mut self, keyword: &str) -> Option<Box<dyn Function>> {
        self.take(keyword)
    }

    pub fn required(&mut self, keyword: &str) -> Result<Box<dyn Function>> {
        self.take(keyword)
            .ok_or(format!("unknown keyword: {}", keyword))
    }

    pub fn keywords(&self) -> Vec<&str> {
        self.keywords.keys().map(String::as_str).collect()
    }

    pub fn len(&self) -> usize {
        self.arguments.len()
    }

    fn take(&mut self, keyword: &str) -> Option<Box<dyn Function>> {
        self.arguments
            .iter()
            .position(|a| a.parameter.keyword == keyword)
            .map(|i| self.arguments.remove(i))
            .map(|v| Box::new(v) as _)
    }
}

pub(in crate::mapping) struct Argument {
    resolver: Box<dyn Function>,
    parameter: Parameter,
}

// delegates to resolver to satisfy tests in `mapping::parser`.
impl std::fmt::Debug for Argument {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.resolver.fmt(f)
    }
}

impl Argument {
    pub fn new(resolver: Box<dyn Function>, parameter: Parameter) -> Self {
        Self {
            resolver,
            parameter,
        }
    }
}

impl Function for Argument {
    fn execute(&self, ctx: &Event) -> Result<QueryValue> {
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
