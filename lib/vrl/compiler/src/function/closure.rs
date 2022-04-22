use value::kind::{Collection, Field, Index};

use crate::value::Kind;
use std::collections::BTreeMap;

use super::Example;

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
