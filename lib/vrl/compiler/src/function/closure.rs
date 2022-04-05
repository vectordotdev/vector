use value::kind::{Collection, Field, Index};

use crate::value::Kind;
use std::collections::BTreeMap;

use super::Example;

/// The definition of a function-closure block a function expects to
/// receive.
#[derive(Debug)]
pub struct Definition {
    pub inputs: Vec<Input>,
}

/// One input variant for a function-closure.
///
/// A closure can support different variable input shapes, depending on the
/// type of a given parameter of the function.
///
/// For example, the `map` function takes either an `Object` or an `Array`
/// for the `value` parameter, and the closure it takes either accepts
/// `|key, value|`, where "key" is always a string, or `|index, value|` where
/// "index" is always a number, depending on the parameter input type.
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
/// a `ClosureVariable`.
#[derive(Debug, Clone)]
pub struct Variable {
    /// The value kind this variable will return when called.
    pub kind: Kind,
}

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

    Scalar {
        /// The expected scalar kind.
        kind: Kind,
    },

    Any,
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
            Output::Scalar { kind } => kind,
            Output::Any => Kind::any(),
        }
    }
}
