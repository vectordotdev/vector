use crate::value::error::Error;
use std::borrow::Cow;
use std::cell::{Ref, RefCell, RefMut};
use std::rc::Rc;

use crate::Value;

mod convert;
mod path;
mod target;

#[derive(Debug, Clone, PartialEq)]
pub struct SharedValue(pub(crate) Rc<RefCell<Value>>);

impl std::hash::Hash for SharedValue {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.borrow().hash(state)
    }
}

impl Eq for SharedValue {}

impl std::fmt::Display for SharedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", *self.borrow())
    }
}

impl SharedValue {
    pub fn borrow(&self) -> Ref<Value> {
        self.0.borrow()
    }

    pub fn borrow_mut(&self) -> RefMut<Value> {
        self.0.borrow_mut()
    }

    pub fn replace(&self, value: Value) -> Value {
        self.0.replace(value)
    }

    pub fn swap(&self, value: &Self) {
        self.0.swap(value.0.as_ref());
    }

    pub fn is_borrowed(&self) -> bool {
        !self.0.try_borrow_mut().is_ok()
    }

    pub fn deep_clone(&self) -> Self {
        SharedValue::from(match &*self.0.borrow() {
            Value::Array(values) => {
                Value::Array(values.iter().map(|value| value.deep_clone()).collect())
            }
            Value::Object(object) => Value::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.deep_clone()))
                    .collect(),
            ),
            value => value.clone(),
        })
    }

    pub fn try_integer(&self) -> Result<i64, crate::value::error::Error> {
        let value = self.borrow();
        value.try_integer()
    }

    pub fn try_boolean(&self) -> Result<bool, crate::value::error::Error> {
        let value = self.borrow();
        value.try_boolean()
    }

    pub fn is_object(&self) -> bool {
        self.borrow().is_object()
    }

    pub fn is_timestamp(&self) -> bool {
        self.borrow().is_timestamp()
    }

    pub fn is_boolean(&self) -> bool {
        self.borrow().is_boolean()
    }

    pub fn is_bytes(&self) -> bool {
        self.borrow().is_bytes()
    }

    pub fn is_float(&self) -> bool {
        self.borrow().is_float()
    }

    pub fn is_integer(&self) -> bool {
        self.borrow().is_integer()
    }

    pub fn is_null(&self) -> bool {
        self.borrow().is_null()
    }

    pub fn is_regex(&self) -> bool {
        self.borrow().is_regex()
    }

    pub fn is_array(&self) -> bool {
        self.borrow().is_array()
    }
}
