use crate::value;
use std::collections::HashMap;

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
#[derive(Debug)]
pub struct Input {
    /// The parameter keyword upon which this closure input variant depends on.
    pub parameter_keyword: &'static str,

    /// The value kind this closure input expects from the parameter.
    pub kind: value::Kind,

    /// The list of variables attached to this closure input type.
    pub variables: Vec<Variable>,

    /// The return type this input variant expects the closure to have.
    pub output: Output,
}

/// One variable input for a closure.
///
/// For example, in `{ |foo, bar| ... }`, `foo` and `bar` are each
/// a `ClosureVariable`.
#[derive(Debug)]
pub struct Variable {
    /// The value kind this variable will return when called.
    pub kind: value::Kind,
}

#[derive(Debug)]
pub enum Output {
    Array {
        /// The number, and kind of elements expected.
        elements: Vec<value::Kind>,
    },

    Object {
        /// The field names, and value kinds expected.
        fields: HashMap<&'static str, value::Kind>,
    },

    Scalar {
        /// The expected scalar kind.
        kind: value::Kind,
    },

    Any,
}
