use crate::expression::Resolved;
use crate::{Context, Expression, State, TypeDef, Value};
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Noop;

impl Expression for Noop {
    fn resolve(&self, _: &mut Context) -> Resolved {
        Ok(Rc::new(RefCell::new(Value::Null)))
    }

    fn type_def(&self, _: &State) -> TypeDef {
        TypeDef::new().null().infallible()
    }
}

impl fmt::Display for Noop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("null")
    }
}
