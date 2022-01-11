//! The `kind` module has all relevant types related to progressive type checking.

mod accessor;
mod builder;
mod collection;
mod comparison;
mod conversion;
pub mod insert;
pub mod merge;

pub use builder::EmptyKindError;
pub use collection::{Collection, Field, Index};

/// The type (kind) of a given value.
///
/// This struct tracks the known states a type can have. By allowing one type to have multiple
/// states, the type definition can be progressively refined.
///
/// At the start, a type is in the "any" state, meaning its type can be any of the valid states, as
/// more information becomes available, states can be removed, until one state is left.
///
/// A state without any type information (e.g. all fields are `None`) indicates no type information
/// can be inferred from the value. This is usually a programming error, but it's a valid state for
/// this library to expose.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Kind {
    bytes: Option<()>,
    integer: Option<()>,
    float: Option<()>,
    boolean: Option<()>,
    timestamp: Option<()>,
    regex: Option<()>,
    null: Option<()>,
    array: Option<Collection<Index>>,
    object: Option<Collection<Field>>,
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_any() {
            return f.write_str("any");
        }

        let mut kinds = vec![];

        if self.is_bytes() {
            kinds.push("string");
        }
        if self.is_integer() {
            kinds.push("integer");
        }
        if self.is_float() {
            kinds.push("float");
        }
        if self.is_boolean() {
            kinds.push("boolean");
        }
        if self.is_timestamp() {
            kinds.push("timestamp");
        }
        if self.is_regex() {
            kinds.push("regex");
        }
        if self.is_null() {
            kinds.push("null");
        }
        if self.is_array() {
            kinds.push("array");
        }
        if self.is_object() {
            kinds.push("object");
        }

        let last = kinds.remove(0);

        if kinds.is_empty() {
            return last.fmt(f);
        }

        let mut kinds = kinds.into_iter().peekable();

        while let Some(kind) = kinds.next() {
            kind.fmt(f)?;

            if kinds.peek().is_some() {
                f.write_str(", ")?;
            }
        }

        f.write_str(" or ")?;
        last.fmt(f)?;

        Ok(())
    }
}
