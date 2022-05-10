use crate::{value::object::Object, Value};
use lookup::{FieldBuf, LookupBuf, SegmentBuf};

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
                    let mut obj = Object::new();
                    obj.insert(name, self);
                    self = Self::Object(obj);
                }
                SegmentBuf::Coalesce(fields) => {
                    let field = fields.last().expect("fields should not be empty");
                    let mut obj = Object::new();
                    obj.insert(field, self);
                    self = Self::Object(obj);
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
    use crate::{value::object::Object, Value};
    use lookup::{parser, LookupBuf};

    #[test]
    fn test_object() {
        let path = parser::parse_lookup(".foo.bar.baz").unwrap();
        let value = Value::Integer(12);

        let bar_value = Value::Object(Object::from([("baz", value.clone())]));
        let foo_value = Value::Object(Object::from([("bar", bar_value)]));

        let object = Value::Object(Object::from([("foo", foo_value)]));

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
        let value = Value::Object([("bar", vec![12].into())].into()); //value!({ "bar": [12] });

        let baz_value = Value::Array(vec![Value::Null, value.clone()]);
        let foo_value = Value::Object(Object::from([("baz", baz_value)]));

        let object = Value::Array(vec![
            Value::Null,
            Value::Null,
            Value::Object(Object::from([("foo", foo_value)])),
        ]);

        assert_eq!(value.at_path(&path.into_buf()), object);
    }
}
