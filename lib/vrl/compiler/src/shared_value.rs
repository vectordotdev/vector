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

    pub fn replace(&self, value: Value) {
        self.0.replace(value);
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
}
