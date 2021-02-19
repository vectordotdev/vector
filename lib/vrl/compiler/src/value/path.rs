use super::Value;
use crate::path::Segment::*;
use crate::Path;
use std::collections::BTreeMap;

impl Value {
    /// Insert the current value into a given path.
    ///
    /// For example, given the path `.foo.bar` and value `true`, the return
    /// value would be an object representing `{ "foo": { "bar": true } }`.
    pub fn at_path(mut self, path: &Path) -> Self {
        for segment in path.segments().iter().rev() {
            match segment {
                Field(field) => {
                    let mut map = BTreeMap::default();
                    map.insert(field.as_str().to_owned(), self);
                    self = Value::Object(map);
                }
                Coalesce(fields) => {
                    let field = fields.last().unwrap();
                    let mut map = BTreeMap::default();
                    map.insert(field.as_str().to_owned(), self);
                    self = Value::Object(map);
                }
                Index(index) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value;
    use std::str::FromStr;

    #[test]
    fn test_dotless() {
        let path = Path::from_str("foo.bar.baz").unwrap();
        let value = value!(12);

        let object = value!({ "foo": { "bar": { "baz": 12 } } });

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_object() {
        let path = Path::from_str(".foo.bar.baz").unwrap();
        let value = value!(12);

        let object = value!({ "foo": { "bar": { "baz": 12 } } });

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_root() {
        let path = Path::from_str(".").unwrap();
        let value = value!(12);

        let object = value!(12);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_array() {
        let path = Path::from_str(".[2]").unwrap();
        let value = value!(12);

        let object = value!([null, null, 12]);

        assert_eq!(value.at_path(&path), object);
    }

    #[test]
    fn test_complex() {
        let path = Path::from_str(".[2].foo.(bar | baz )[1]").unwrap();
        let value = value!({ "bar": [12] });

        let object = value!([null, null, { "foo": { "baz": [null, { "bar": [12] }] } } ]);

        assert_eq!(value.at_path(&path), object);
    }
}
