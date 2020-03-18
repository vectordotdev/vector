use super::{Atom, PathComponent, PathIter, Value};
use std::{cmp::Ordering, collections::BTreeMap, iter::Peekable, mem};

/// Removes field value specified by the given path and return its value.
///
/// A special case worth mentioning: if there is a nested array and an item is removed
/// from the middle of this array, then it is just replaced by `Value::Null`.
pub fn remove(fields: &mut BTreeMap<Atom, Value>, path: &str) -> Option<Value> {
    remove_map(fields, PathIter::new(path).peekable())
}

/// Recursively iterate through the path, and remove the last path
/// element. This is the top-level function which can remove from any
/// type of `Value`.
fn remove_rec(value: &mut Value, path: Peekable<PathIter>) -> Option<Value> {
    match value {
        Value::Map(map) => remove_map(map, path),
        Value::Array(map) => remove_array(map, path),
        _ => None,
    }
}

fn remove_array(array: &mut Vec<Value>, mut path: Peekable<PathIter>) -> Option<Value> {
    match path.next()? {
        PathComponent::Index(index) => match path.peek() {
            None => return array_remove(array, index),
            Some(_) => array
                .get_mut(index)
                .and_then(|value| remove_rec(value, path)),
        },
        _ => return None,
    }
}

fn remove_map(fields: &mut BTreeMap<Atom, Value>, mut path: Peekable<PathIter>) -> Option<Value> {
    match path.next()? {
        PathComponent::Key(key) => match path.peek() {
            None => fields.remove(&key),
            Some(_) => fields
                .get_mut(&key)
                .and_then(|value| remove_rec(value, path)),
        },
        _ => return None,
    }
}

fn array_remove(values: &mut Vec<Value>, index: usize) -> Option<Value> {
    match (index + 1).cmp(&values.len()) {
        Ordering::Less => Some(mem::replace(&mut values[index], Value::Null)),
        Ordering::Equal => values.pop(),
        Ordering::Greater => None,
    }
}

#[cfg(test)]
mod test {
    use super::super::test::fields_from_json;
    use super::*;
    use serde_json::json;

    #[test]
    fn array_remove_from_middle() {
        let mut array = vec![Value::Null, Value::Integer(3)];
        assert_eq!(array_remove(&mut array, 0), Some(Value::Null));
        assert_eq!(array_remove(&mut array, 0), Some(Value::Null));

        assert_eq!(array_remove(&mut array, 1), Some(Value::Integer(3)));
        assert_eq!(array_remove(&mut array, 1), None);

        assert_eq!(array_remove(&mut array, 0), Some(Value::Null));
        assert_eq!(array_remove(&mut array, 0), None);
    }

    #[test]
    fn remove_simple() {
        let mut fields = fields_from_json(json!({
            "field": 123
        }));
        assert_eq!(remove(&mut fields, "field"), Some(Value::Integer(123)));
        assert_eq!(remove(&mut fields, "field"), None);
    }

    #[test]
    fn remove_nested() {
        let mut fields = fields_from_json(json!({
            "a": {
                "b": {
                    "c": 5
                },
                "d": 4,
                "array": [null, 3, {
                    "x": 1
                }, [2]]
            }
        }));
        let queries = [
            ("a.b.c", Some(Value::Integer(5)), None),
            ("a.d", Some(Value::Integer(4)), None),
            ("a.array[1]", Some(Value::Integer(3)), Some(Value::Null)),
            ("a.array[2].x", Some(Value::Integer(1)), None),
            ("a.array[3][0]", Some(Value::Integer(2)), None),
            ("a.array[3][1]", None, None),
            ("a.x", None, None),
            ("z", None, None),
            (".123", None, None),
            ("", None, None),
        ];

        for (query, expected_first, expected_second) in queries.iter() {
            assert_eq!(remove(&mut fields, query), *expected_first, "{}", query);
            assert_eq!(remove(&mut fields, query), *expected_second, "{}", query);
        }
    }
}
