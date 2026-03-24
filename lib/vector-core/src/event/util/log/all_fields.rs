use std::{collections::btree_map, iter, slice};

use lookup::{OwnedTargetPath, OwnedValuePath};
use serde::{Serialize, Serializer};
use vrl::{
    owned_value_path,
    path::{OwnedSegment, PathPrefix},
};

use crate::event::{KeyString, ObjectMap, Value};

/// Iterates over all paths in form `a.b[0].c[1]` in alphabetical order
/// and their corresponding values.
pub fn all_fields(fields: &ObjectMap) -> FieldsIter<'_> {
    FieldsIter::new(PathPrefix::Event, fields)
}

/// Same functionality as `all_fields` but it prepends a character that denotes the
/// path type.
pub fn all_metadata_fields(fields: &ObjectMap) -> FieldsIter<'_> {
    FieldsIter::new(PathPrefix::Metadata, fields)
}

/// An iterator with a single "message" element
pub fn all_fields_non_object_root(value: &Value) -> FieldsIter<'_> {
    FieldsIter::non_object(value)
}

/// An iterator similar to `all_fields`, but instead of visiting each array element individually,
/// it treats the entire array as a single value.
pub fn all_fields_skip_array_elements(fields: &ObjectMap) -> FieldsIter<'_> {
    FieldsIter::new_with_skip_array_elements(fields)
}

#[derive(Clone, Debug)]
enum LeafIter<'a> {
    Root((&'a Value, bool)),
    Map(btree_map::Iter<'a, KeyString, Value>),
    Array(iter::Enumerate<slice::Iter<'a, Value>>),
}

#[derive(Clone, Copy, Debug)]
enum PathComponent<'a> {
    Key(&'a KeyString),
    Index(usize),
}

/// Performs depth-first traversal of the nested structure.
///
/// If a key maps to an empty collection, the key and the empty collection will be returned.
#[derive(Clone)]
pub struct FieldsIter<'a> {
    /// If specified, this will be prepended to each path.
    path_prefix: PathPrefix,
    /// Stack of iterators used for the depth-first traversal.
    stack: Vec<LeafIter<'a>>,
    /// Path components from the root up to the top of the stack.
    path: Vec<PathComponent<'a>>,
    /// Treat array as a single value and don't traverse each element.
    skip_array_elements: bool,
}

impl<'a> FieldsIter<'a> {
    fn new(path_prefix: PathPrefix, fields: &'a ObjectMap) -> FieldsIter<'a> {
        FieldsIter {
            path_prefix,
            stack: vec![LeafIter::Map(fields.iter())],
            path: vec![],
            skip_array_elements: false,
        }
    }

    /// This is for backwards compatibility. An event where the root is not an object
    /// will be treated as an object with a single "message" key
    fn non_object(value: &'a Value) -> FieldsIter<'a> {
        FieldsIter {
            path_prefix: PathPrefix::Event,
            stack: vec![LeafIter::Root((value, false))],
            path: vec![],
            skip_array_elements: false,
        }
    }

    fn new_with_skip_array_elements(fields: &'a ObjectMap) -> FieldsIter<'a> {
        FieldsIter {
            path_prefix: PathPrefix::Event,
            stack: vec![LeafIter::Map(fields.iter())],
            path: vec![],
            skip_array_elements: true,
        }
    }

    fn push(&mut self, value: &'a Value, component: PathComponent<'a>) -> Option<&'a Value> {
        match value {
            Value::Object(map) if !map.is_empty() => {
                self.stack.push(LeafIter::Map(map.iter()));
                self.path.push(component);
                None
            }
            Value::Array(array) if !array.is_empty() => {
                if self.skip_array_elements {
                    Some(value)
                } else {
                    self.stack.push(LeafIter::Array(array.iter().enumerate()));
                    self.path.push(component);
                    None
                }
            }
            _ => Some(value),
        }
    }

    fn pop(&mut self) {
        self.stack.pop();
        self.path.pop();
    }

    fn make_path(&mut self, component: PathComponent<'a>) -> OwnedTargetPath {
        let segments = self
            .path
            .iter()
            .chain(iter::once(&component))
            .map(|val| match val {
                PathComponent::Key(key_string) => OwnedSegment::Field((*key_string).to_owned()),
                PathComponent::Index(idx) => OwnedSegment::Index(*idx as isize),
            })
            .collect();

        OwnedTargetPath {
            prefix: self.path_prefix,
            path: OwnedValuePath { segments },
        }
    }
}

impl<'a> Iterator for FieldsIter<'a> {
    type Item = (OwnedTargetPath, &'a Value);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.stack.last_mut() {
                None => return None,
                Some(LeafIter::Map(map_iter)) => match map_iter.next() {
                    None => self.pop(),
                    Some((key, value)) => {
                        if let Some(scalar_value) = self.push(value, PathComponent::Key(key)) {
                            return Some((self.make_path(PathComponent::Key(key)), scalar_value));
                        }
                    }
                },
                Some(LeafIter::Array(array_iter)) => match array_iter.next() {
                    None => self.pop(),
                    Some((index, value)) => {
                        if let Some(scalar_value) = self.push(value, PathComponent::Index(index)) {
                            return Some((
                                self.make_path(PathComponent::Index(index)),
                                scalar_value,
                            ));
                        }
                    }
                },
                Some(LeafIter::Root((value, visited))) => {
                    let result = (!*visited)
                        .then(|| (OwnedTargetPath::event(owned_value_path!("message")), *value));

                    *visited = true;
                    break result;
                }
            }
        }
    }
}

impl Serialize for FieldsIter<'_> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_map(self.clone().map(|(key, val)| {
            let key_str = match key.prefix {
                PathPrefix::Event => key.path.to_string(), // -> not .field but just field
                PathPrefix::Metadata => key.to_string(),   // metadata keeps % prepended (%field)
            };

            (key_str, val)
        }))
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;
    use similar_asserts::assert_eq;

    use super::{super::test::fields_from_json, *};

    #[test]
    fn keys_simple() {
        let fields = fields_from_json(json!({
            "field2": 3,
            "field1": 4,
            "field3": 5
        }));
        let expected: Vec<_> = vec![
            ("field1", &Value::Integer(4)),
            ("field2", &Value::Integer(3)),
            ("field3", &Value::Integer(5)),
        ]
        .into_iter()
        .map(|(k, v)| (OwnedTargetPath::event(owned_value_path!(k)), v))
        .collect();

        let collected: Vec<_> = all_fields(&fields).collect();
        assert_eq!(collected, expected);
    }

    fn special_fields() -> ObjectMap {
        fields_from_json(json!({
                    "a-b": 1,
                    "a*b": 2,
                    "a b": 3,
                    ".a .b*": 4,
                    "\"a\"": 5,
        }))
    }

    #[test]
    fn keys_special() {
        let fields = special_fields();
        let mut collected: Vec<_> = all_fields(&fields).collect();
        collected.sort_by(|(a, _), (b, _)| a.cmp(b));

        let mut expected: Vec<(OwnedTargetPath, &Value)> = vec![
            ("a-b", &Value::Integer(1)),
            ("a*b", &Value::Integer(2)),
            ("a b", &Value::Integer(3)),
            (".a .b*", &Value::Integer(4)),
            ("\"a\"", &Value::Integer(5)),
        ]
        .into_iter()
        .map(|(k, v)| (OwnedTargetPath::event(owned_value_path!(k)), v))
        .collect();

        expected.sort_by(|(a, _), (b, _)| a.cmp(b));

        assert_eq!(collected, expected);
    }

    #[test]
    fn metadata_keys_simple() {
        let fields = fields_from_json(json!({
            "field_1": 1,
            "field_0": 0,
            "field_2": 2
        }));
        let expected: Vec<_> = vec![
            ("field_0", &Value::Integer(0)),
            ("field_1", &Value::Integer(1)),
            ("field_2", &Value::Integer(2)),
        ]
        .into_iter()
        .map(|(k, v)| (OwnedTargetPath::metadata(owned_value_path!(k)), v))
        .collect();

        let collected: Vec<_> = all_metadata_fields(&fields).collect();

        assert_eq!(collected, expected);
    }

    fn nested_fields() -> ObjectMap {
        fields_from_json(json!({
                    "a": {
                        "b": {
                            "c": 5
                        },
                        "a": 4,
                        "array": [null, 3, {
                            "x": 1
                        }, [2]]
                    },
                    "a.b.c": 6,
                    "d": {},
                    "e": [],
        }))
    }

    #[test]
    fn keys_nested() {
        let fields = nested_fields();
        let expected: Vec<_> = vec![
            (owned_value_path!("a", "a"), Value::Integer(4)),
            (owned_value_path!("a", "array", 0), Value::Null),
            (owned_value_path!("a", "array", 1), Value::Integer(3)),
            (owned_value_path!("a", "array", 2, "x"), Value::Integer(1)),
            (owned_value_path!("a", "array", 3, 0), Value::Integer(2)),
            (owned_value_path!("a", "b", "c"), Value::Integer(5)),
            (owned_value_path!("a.b.c"), Value::Integer(6)),
            (owned_value_path!("d"), Value::Object(ObjectMap::new())),
            (owned_value_path!("e"), Value::Array(Vec::new())),
        ]
        .into_iter()
        .map(|(k, v)| (OwnedTargetPath::event(k), v))
        .collect();

        let collected: Vec<_> = all_fields(&fields).map(|(k, v)| (k, v.clone())).collect();
        assert_eq!(collected, expected);
    }

    #[test]
    fn test_non_object_root() {
        let value = Value::Integer(3);
        let collected: Vec<_> = all_fields_non_object_root(&value)
            .map(|(k, v)| (k, v.clone()))
            .collect();

        assert_eq!(
            collected,
            vec![(OwnedTargetPath::event(owned_value_path!("message")), value)]
        );
    }
}
