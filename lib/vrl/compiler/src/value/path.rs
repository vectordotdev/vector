use super::Value;
use lookup::{FieldBuf, LookupBuf, SegmentBuf};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

impl Value {
    /// Insert the current value into a given path.
    ///
    /// For example, given the path `.foo.bar` and value `true`, the return
    /// value would be an object representing `{ "foo": { "bar": true } }`.
    pub fn at_path(me: Rc<RefCell<Self>>, path: &LookupBuf) -> Rc<RefCell<Self>> {
        for segment in path.as_segments().iter().rev() {
            match segment {
                SegmentBuf::Field(FieldBuf { name, .. }) => {
                    let mut map = BTreeMap::default();
                    map.insert(name.as_str().to_owned(), Rc::clone(&me));
                    me.replace(Value::Object(map));
                }
                SegmentBuf::Coalesce(fields) => {
                    let field = fields.last().unwrap();
                    let mut map = BTreeMap::default();
                    map.insert(field.as_str().to_owned(), Rc::clone(&me));
                    me.replace(Value::Object(map));
                }
                SegmentBuf::Index(index) => {
                    let mut array = vec![];

                    if *index > 0 {
                        array.resize(*index as usize, Rc::new(RefCell::new(Value::Null)));
                    }

                    array.push(Rc::clone(&me));
                    me.replace(Value::Array(array));
                }
            }
        }

        me
    }
}

#[cfg(test)]
mod tests {
    use crate::value;

    #[test]
    fn test_object() {
        let path = parser::parse_path(".foo.bar.baz").unwrap();
        let value = value!(12);

        let object = value!({ "foo": { "bar": { "baz": 12 } } });

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_root() {
        let path = parser::parse_path(".").unwrap();
        let value = value!(12);

        let object = value!(12);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_array() {
        let path = parser::parse_path(".[2]").unwrap();
        let value = value!(12);

        let object = value!([null, null, 12]);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_complex() {
        let path = parser::parse_path(".[2].foo.(bar | baz )[1]").unwrap();
        let value = value!({ "bar": [12] });

        let object = value!([null, null, { "foo": { "baz": [null, { "bar": [12] }] } } ]);

        assert_eq!(value.at_path(&path), object);
    }
}
