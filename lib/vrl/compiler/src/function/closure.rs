use core::ExpressionError;
use std::collections::BTreeMap;

use parser::ast::Ident;
use value::{
    kind::{Collection, Field, Index},
    Value,
};

use super::Example;
use crate::{
    state::Runtime,
    value::{Kind, VrlValueConvert},
    Context,
};

/// The definition of a function-closure block a function expects to
/// receive.
#[derive(Debug)]
pub struct Definition {
    /// A list of input configurations valid for this closure definition.
    pub inputs: Vec<Input>,

    /// Defines whether the closure is expected to iterate over the elements of
    /// a collection.
    ///
    /// If this is `true`, the compiler will (1) reject any non-iterable types
    /// passed to this closure, and (2) use the type definition of inner
    /// collection elements to determine the eventual type definition of the
    /// closure variable(s) (see `Variable`).
    pub is_iterator: bool,
}

/// One input variant for a function-closure.
///
/// A closure can support different variable input shapes, depending on the
/// type of a given parameter of the function.
///
/// For example, the `for_each` function takes either an `Object` or an `Array`
/// for the `value` parameter, and the closure it takes either accepts `|key,
/// value|`, where "key" is always a string, or `|index, value|` where "index"
/// is always a number, depending on the parameter input type.
#[derive(Debug, Clone)]
pub struct Input {
    /// The parameter keyword upon which this closure input variant depends on.
    pub parameter_keyword: &'static str,

    /// The value kind this closure input expects from the parameter.
    pub kind: Kind,

    /// The list of variables attached to this closure input type.
    pub variables: Vec<Variable>,

    /// The return type this input variant expects the closure to have.
    pub output: Output,

    /// An example matching the given input.
    pub example: Example,
}

/// One variable input for a closure.
///
/// For example, in `{ |foo, bar| ... }`, `foo` and `bar` are each
/// a `Variable`.
#[derive(Debug, Clone)]
pub struct Variable {
    /// The value kind this variable will return when called.
    ///
    /// If set to `None`, the compiler is expected to provide this value at
    /// compile-time, or resort to `Kind::any()` if no information is known.
    pub kind: VariableKind,
}

/// The [`Value`] kind expected to be returned by a [`Variable`].
#[derive(Debug, Clone)]
pub enum VariableKind {
    /// An exact [`Kind`] means this variable is guaranteed to always contain
    /// a value that resolves to this kind.
    ///
    /// For example, in `map_keys`, it is known that the first (and only)
    /// variable the closure takes will be a `Kind::bytes()`.
    Exact(Kind),

    /// The variable [`Kind`] is inferred from the target of the closure.
    Target,

    /// The variable [`Kind`] is inferred from the inner kind of the target of
    /// the closure. This requires the closure target to be a collection type.
    TargetInnerValue,

    /// The variable [`Kind`] is inferred from the key or index type of the
    /// target. If the target is known to be exactly an object, this is always
    /// a `Value::bytes()`, if it's known to be exactly an array, it is
    /// a `Value::integer()`, otherwise it is one of the two.
    TargetInnerKey,
}

/// The output type required by the closure block.
#[derive(Debug, Clone)]
pub enum Output {
    Array {
        /// The number, and kind of elements expected.
        elements: Vec<Kind>,
    },

    Object {
        /// The field names, and value kinds expected.
        fields: BTreeMap<&'static str, Kind>,
    },

    Kind(
        /// The expected kind.
        Kind,
    ),
}

impl Output {
    pub fn into_kind(self) -> Kind {
        match self {
            Output::Array { elements } => {
                let collection: Collection<Index> = elements
                    .into_iter()
                    .enumerate()
                    .map(|(i, k)| (i.into(), k))
                    .collect::<BTreeMap<_, _>>()
                    .into();

                collection.into()
            }
            Output::Object { fields } => {
                let collection: Collection<Field> = fields
                    .into_iter()
                    .map(|(k, v)| (k.into(), v))
                    .collect::<BTreeMap<_, _>>()
                    .into();

                collection.into()
            }
            Output::Kind(kind) => kind,
        }
    }
}

pub struct Runner<'a, T> {
    pub(crate) variables: &'a [Ident],
    pub(crate) runner: T,
}

impl<'a, T> Runner<'a, T>
where
    T: Fn(&mut Context) -> Result<Value, ExpressionError>,
{
    pub fn new(variables: &'a [Ident], runner: T) -> Self {
        Self { variables, runner }
    }

    /// Run the closure to completion, given the provided key/value pair, and
    /// the runtime context.
    ///
    /// The provided values are *NOT* mutated during the run, and no return
    /// value is provided by this method. See `map_key` or `map_value` for
    /// mutating alternatives.
    pub fn run_key_value(
        &self,
        ctx: &mut Context,
        key: &str,
        value: &Value,
    ) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_key = key.to_owned();
        let cloned_value = value.clone();

        let key_ident = self.ident(0);
        let value_ident = self.ident(1);

        let old_key = insert(ctx.state_mut(), key_ident, cloned_key.into());
        let old_value = insert(ctx.state_mut(), value_ident, cloned_value);

        (self.runner)(ctx)?;

        cleanup(ctx.state_mut(), key_ident, old_key);
        cleanup(ctx.state_mut(), value_ident, old_value);

        Ok(())
    }

    /// Run the closure to completion, given the provided index/value pair, and
    /// the runtime context.
    ///
    /// The provided values are *NOT* mutated during the run, and no return
    /// value is provided by this method. See `map_key` or `map_value` for
    /// mutating alternatives.
    pub fn run_index_value(
        &self,
        ctx: &mut Context,
        index: usize,
        value: &Value,
    ) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_value = value.clone();

        let index_ident = self.ident(0);
        let value_ident = self.ident(1);

        let old_index = insert(ctx.state_mut(), index_ident, index.into());
        let old_value = insert(ctx.state_mut(), value_ident, cloned_value);

        (self.runner)(ctx)?;

        cleanup(ctx.state_mut(), index_ident, old_index);
        cleanup(ctx.state_mut(), value_ident, old_value);

        Ok(())
    }

    /// Run the closure to completion, given the provided key, and the runtime
    /// context.
    ///
    /// The provided key is *MUTATED* by overwriting the key with the return
    /// value of the closure after completion.
    ///
    /// See `run_key_value` and `run_index_value` for immutable alternatives.
    pub fn map_key(&self, ctx: &mut Context, key: &mut String) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_key = key.clone();
        let ident = self.ident(0);
        let old_key = insert(ctx.state_mut(), ident, cloned_key.into());

        *key = (self.runner)(ctx)?.try_bytes_utf8_lossy()?.into_owned();

        cleanup(ctx.state_mut(), ident, old_key);

        Ok(())
    }

    /// Run the closure to completion, given the provided value, and the runtime
    /// context.
    ///
    /// The provided value is *MUTATED* by overwriting the value with the return
    /// value of the closure after completion.
    ///
    /// See `run_key_value` and `run_index_value` for immutable alternatives.
    pub fn map_value(&self, ctx: &mut Context, value: &mut Value) -> Result<(), ExpressionError> {
        // TODO: we need to allow `LocalEnv` to take a muable reference to
        // values, instead of owning them.
        let cloned_value = value.clone();
        let ident = self.ident(0);
        let old_value = insert(ctx.state_mut(), ident, cloned_value);

        *value = (self.runner)(ctx)?;

        cleanup(ctx.state_mut(), ident, old_value);

        Ok(())
    }

    fn ident(&self, index: usize) -> Option<&Ident> {
        self.variables
            .get(index)
            .and_then(|v| (!v.is_empty()).then(|| v))
    }
}

fn insert(state: &mut Runtime, ident: Option<&Ident>, data: Value) -> Option<Value> {
    ident.and_then(|ident| state.swap_variable(ident.clone(), data))
}

fn cleanup(state: &mut Runtime, ident: Option<&Ident>, data: Option<Value>) {
    match (ident, data) {
        (Some(ident), Some(value)) => {
            state.insert_variable(ident.clone(), value);
        }
        (Some(ident), None) => state.remove_variable(ident),
        _ => {}
    }
}
