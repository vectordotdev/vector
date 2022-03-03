use crate::Value;
use lookup::{FieldBuf, LookupBuf, SegmentBuf};
use std::collections::BTreeMap;

impl Value {
    /// Insert the current value into a given path.
    ///
    /// For example, given the path `.foo.bar` and value `true`, the return
    /// value would be an object representing `{ "foo": { "bar": true } }`.
    #[must_use]
    pub fn at_path(mut self, path: &LookupBuf) -> Self {
        for segment in path.as_segments().iter().rev() {
            match segment {
                SegmentBuf::Field(FieldBuf { name, .. }) => {
                    let mut map = BTreeMap::default();
                    map.insert(name.as_str().to_owned(), self);
                    self = Self::Object(map);
                }
                SegmentBuf::Coalesce(fields) => {
                    let field = fields.last().expect("fields should not be empty");
                    let mut map = BTreeMap::default();
                    map.insert(field.as_str().to_owned(), self);
                    self = Self::Object(map);
                }
                SegmentBuf::Index(index) => {
                    let mut array = vec![];

                    if *index > 0 {
                        array.resize(*index as usize, Self::Null);
                    }

                    array.push(self);
                    self = Self::Array(array);
                }
            }
        }

        self
    }
}

#[cfg(test)]
mod tests {
    use crate::Value;
    use lookup::{parser, LookupBuf};
    use std::collections::BTreeMap;

    #[test]
    fn test_object() {
        let path = parser::parse_lookup(".foo.bar.baz").unwrap();
        let value = Value::Integer(12);

        let bar_value = Value::Object(BTreeMap::from([("baz".into(), value.clone())]));
        let foo_value = Value::Object(BTreeMap::from([("bar".into(), bar_value)]));

        let object = Value::Object(BTreeMap::from([("foo".into(), foo_value)]));

        assert_eq!(value.at_path(&path.into_buf()), object);
    }

    #[test]
    fn test_root() {
        let path = LookupBuf::default();
        let value = Value::Integer(12);

        let object = Value::Integer(12);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_array() {
        let path = parser::parse_lookup("[2]").unwrap();
        let value = Value::Integer(12);

        let object = Value::Array(vec![Value::Null, Value::Null, Value::Integer(12)]);

        assert_eq!(value.at_path(&path.into_buf()), object);
    }

    #[test]
    fn test_complex() {
        let path = parser::parse_lookup("[2].foo.(bar | baz )[1]").unwrap();
        let value = Value::Object([("bar".into(), vec![12].into())].into()); //value!({ "bar": [12] });

        let baz_value = Value::Array(vec![Value::Null, value.clone()]);
        let foo_value = Value::Object(BTreeMap::from([("baz".into(), baz_value)]));

        let object = Value::Array(vec![
            Value::Null,
            Value::Null,
            Value::Object(BTreeMap::from([("foo".into(), foo_value)])),
        ]);

        assert_eq!(value.at_path(&path.into_buf()), object);
    }
}
