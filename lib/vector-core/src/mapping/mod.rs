use std::{collections::BTreeMap, convert::TryFrom};

use crate::event::{Event, Value};

#[allow(clippy::upper_case_acronyms)]
// some of the generated names, like NEWLINE, come from Pest itself https://github.com/pest-parser/pest/issues/49k0
pub mod parser;
pub mod query;

use query::query_value::QueryValue;

pub type Result<T> = std::result::Result<T, String>;

pub(self) trait Function: Send + core::fmt::Debug {
    fn apply(&self, target: &mut Event) -> Result<()>;
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Assignment {
    path: String,
    function: Box<dyn query::Function>,
}

impl Assignment {
    pub(self) fn new(path: String, function: Box<dyn query::Function>) -> Self {
        Self { path, function }
    }
}

impl Function for Assignment {
    fn apply(&self, target: &mut Event) -> Result<()> {
        match self.function.execute(target)? {
            QueryValue::Value(v) => {
                target.as_mut_log().insert(&self.path, v);
                Ok(())
            }
            _ => Err("assignment must be from a value".to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Deletion {
    paths: Vec<String>,
}

impl Deletion {
    pub(self) fn new(mut paths: Vec<String>) -> Self {
        Self {
            paths: paths.drain(..).collect(),
        }
    }
}

impl Function for Deletion {
    fn apply(&self, target: &mut Event) -> Result<()> {
        for path in &self.paths {
            target.as_mut_log().remove(&path);
        }
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct OnlyFields {
    paths: Vec<String>,
}

impl OnlyFields {
    pub(self) fn new(paths: Vec<String>) -> Self {
        Self { paths }
    }
}

impl Function for OnlyFields {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let target_log = target.as_mut_log();

        let keys: Vec<String> = target_log
            .keys()
            .filter(|k| !self.paths.iter().any(|p| k.starts_with(p.as_str())))
            .collect();

        for key in keys {
            target_log.remove_prune(key, true);
        }

        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct IfStatement {
    query: Box<dyn query::Function>,
    true_statement: Box<dyn Function>,
    false_statement: Box<dyn Function>,
}

impl IfStatement {
    pub(self) fn new(
        query: Box<dyn query::Function>,
        true_statement: Box<dyn Function>,
        false_statement: Box<dyn Function>,
    ) -> Self {
        Self {
            query,
            true_statement,
            false_statement,
        }
    }
}

impl Function for IfStatement {
    fn apply(&self, target: &mut Event) -> Result<()> {
        match self.query.execute(target)? {
            QueryValue::Value(Value::Boolean(true)) => self.true_statement.apply(target),
            QueryValue::Value(Value::Boolean(false)) => self.false_statement.apply(target),
            _ => Err("query returned non-boolean value".to_string()),
        }
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub(self) struct Noop {}

impl Function for Noop {
    fn apply(&self, _: &mut Event) -> Result<()> {
        Ok(())
    }
}

//------------------------------------------------------------------------------

#[derive(Debug)]
pub struct Mapping {
    // this whole module needs to go away but I had trouble untangling it from what is _actually_ used by the legacy lookups
    #[allow(dead_code)]
    assignments: Vec<Box<dyn Function>>,
}

impl Mapping {
    pub(self) fn new(assignments: Vec<Box<dyn Function>>) -> Self {
        Mapping { assignments }
    }
}

//------------------------------------------------------------------------------

/// Merges two `BTreeMap`s of `Value`s.
/// The second map is merged into the first one.
///
/// If `deep` is true, only the top level values are merged in. If both maps
/// contain a field with the same name, the field from the first is overwritten
/// with the field from the second.
///
/// If `deep` is false, should both maps contain a field with the same name, and
/// both those fields are also maps, the function will recurse and will merge
/// the child fields from the second into the child fields from the first.
///
/// Note, this does recurse, so there is the theoretical possibility that it
/// could blow up the stack. From quick tests on a sample project I was able to
/// merge maps with a depth of 3,500 before encountering issues. So I think that
/// is likely to be within acceptable limits.  If it becomes a problem, we can
/// unroll this function, but that will come at a cost of extra code complexity.
fn merge_maps<K>(map1: &mut BTreeMap<K, Value>, map2: &BTreeMap<K, Value>, deep: bool)
where
    K: std::cmp::Ord + Clone,
{
    for (key2, value2) in map2.iter() {
        match (deep, map1.get_mut(key2), value2) {
            (true, Some(Value::Map(ref mut child1)), Value::Map(ref child2)) => {
                // We are doing a deep merge and both fields are maps.
                merge_maps(child1, child2, deep);
            }
            _ => {
                map1.insert(key2.clone(), value2.clone());
            }
        }
    }
}

#[derive(Debug)]
pub(in crate::mapping) struct MergeFn {
    to_path: String,
    from: Box<dyn query::Function>,
    deep: Option<Box<dyn query::Function>>,
}

impl MergeFn {
    pub(in crate::mapping) fn new(
        to_path: String,
        from: Box<dyn query::Function>,
        deep: Option<Box<dyn query::Function>>,
    ) -> Self {
        MergeFn {
            to_path,
            from,
            deep,
        }
    }
}

impl Function for MergeFn {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let from_value = self.from.execute(target)?;
        let deep = match &self.deep {
            None => false,
            Some(deep) => match deep.execute(target)? {
                QueryValue::Value(Value::Boolean(value)) => value,
                _ => return Err("deep parameter passed to merge is a non-boolean value".into()),
            },
        };

        let to_value = target.as_mut_log().get_mut(&self.to_path).ok_or(format!(
            "parameter {} passed to merge is not found",
            self.to_path
        ))?;

        match (to_value, from_value) {
            (Value::Map(ref mut map1), QueryValue::Value(Value::Map(ref map2))) => {
                merge_maps(map1, map2, deep);
                Ok(())
            }

            _ => Err("parameters passed to merge are non-map values".into()),
        }
    }
}

//------------------------------------------------------------------------------

/// Represents the different log levels that can be used by `LogFn`
#[derive(Debug, Clone, Copy)]
pub(in crate::mapping) enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl TryFrom<&str> for LogLevel {
    type Error = String;

    fn try_from(level: &str) -> Result<Self> {
        match level {
            "trace" => Ok(Self::Trace),
            "debug" => Ok(Self::Debug),
            "info" => Ok(Self::Info),
            "warn" => Ok(Self::Warn),
            "error" => Ok(Self::Error),
            _ => Err("invalid log level".to_string()),
        }
    }
}

#[derive(Debug)]
pub(in crate::mapping) struct LogFn {
    msg: Box<dyn query::Function>,
    level: Option<LogLevel>,
}

impl LogFn {
    pub(in crate::mapping) fn new(msg: Box<dyn query::Function>, level: Option<LogLevel>) -> Self {
        Self { msg, level }
    }
}

impl Function for LogFn {
    fn apply(&self, target: &mut Event) -> Result<()> {
        let msg = match self.msg.execute(target)? {
            QueryValue::Value(value) => value,
            _ => return Err("Can only log Value parameters".to_string()),
        };
        let msg = msg.into_bytes();
        let string = String::from_utf8_lossy(&msg);
        let level = self.level.unwrap_or(LogLevel::Info);

        match level {
            LogLevel::Trace => trace!("{}", string),
            LogLevel::Debug => debug!("{}", string),
            LogLevel::Info => info!("{}", string),
            LogLevel::Warn => warn!("{}", string),
            LogLevel::Error => error!("{}", string),
        }

        Ok(())
    }
}
