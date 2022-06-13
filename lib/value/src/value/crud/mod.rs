use crate::Value;
use std::collections::BTreeMap;

mod insert;

pub use self::insert::insert;

pub trait ValueCollection {
    type Key;

    fn get_mut_value(&mut self, key: &Self::Key) -> Option<&mut Value>;
    fn insert_value(&mut self, key: Self::Key, value: Value) -> Option<Value>;
}

impl ValueCollection for Value {
    type Key = ();

    fn get_mut_value(&mut self, key: &()) -> Option<&mut Value> {
        Some(self)
    }

    fn insert_value(&mut self, key: (), value: Value) -> Option<Value> {
        Some(std::mem::replace(self, value))
    }
}

impl ValueCollection for BTreeMap<String, Value> {
    type Key = String;

    fn get_mut_value(&mut self, key: &Self::Key) -> Option<&mut Value> {
        self.get_mut(key)
    }

    fn insert_value(&mut self, key: Self::Key, value: Value) -> Option<Value> {
        self.insert(key, value)
    }
}

impl ValueCollection for Vec<Value> {
    type Key = isize;

    fn get_mut_value(&mut self, key: &isize) -> Option<&mut Value> {
        if *key >= 0 {
            self.get_mut(*key as usize)
        } else {
            unimplemented!()
        }
    }

    fn insert_value(&mut self, key: isize, value: Value) -> Option<Value> {
        if key >= 0 {
            if self.len() <= (key as usize) {
                while self.len() <= (key as usize) {
                    self.push(Value::Null);
                }
                self[key as usize] = value;
                None
            } else {
                Some(std::mem::replace(&mut self[key as usize], value))
            }
        } else {
            let len_required = -key as usize;
            if self.len() < len_required {
                while self.len() < (len_required - 1) {
                    self.insert(0, Value::Null);
                }
                self.insert(0, value);
                None
            } else {
                let index = (self.len() as isize + key) as usize;
                Some(std::mem::replace(&mut self[index], value))
            }
        }
    }
}
