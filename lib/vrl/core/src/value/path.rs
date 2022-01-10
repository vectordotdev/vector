use std::collections::BTreeMap;

use lookup::{FieldBuf, LookupBuf, SegmentBuf};

use crate::value::Value;

impl Value {
    /// Insert the current value into a given path.
    ///
    /// For example, given the path `.foo.bar` and value `true`, the return
    /// value would be an object representing `{ "foo": { "bar": true } }`.
    pub fn at_path(mut self, path: &LookupBuf) -> Self {
        for segment in path.as_segments().iter().rev() {
            match segment {
                SegmentBuf::Field(FieldBuf { name, .. }) => {
                    let mut map = BTreeMap::default();
                    map.insert(name.as_str().to_owned(), self);
                    self = Value::Object(map);
                }
                SegmentBuf::Coalesce(fields) => {
                    let field = fields.last().unwrap();
                    let mut map = BTreeMap::default();
                    map.insert(field.as_str().to_owned(), self);
                    self = Value::Object(map);
                }
                SegmentBuf::Index(index) => {
                    let mut array = vec![];

                    if *index > 0 {
                        array.resize(*index as usize, Value::Null);
                    }

                    array.push(self);
                    self = Value::Array(array);
                }
            }
        }

        self
    }
}
