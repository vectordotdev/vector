use std::{cmp::Ordering, collections::BTreeMap, iter::Peekable, mem};

use super::{PathComponent, PathIter, Value};

/// Removes field value specified by the given path and return its value.
///
/// A special case worth mentioning: if there is a nested array and an item is removed
/// from the middle of this array, then it is just replaced by `Value::Null`.
pub fn remove(fields: &mut BTreeMap<String, Value>, path: &str, prune: bool) -> Option<Value> {
    remove_map(fields, PathIter::new(path).peekable(), prune).map(|(value, _)| value)
}

/// Recursively iterate through the path, and remove the last path
/// element. This is the top-level function which can remove from any
/// type of `Value`.
fn remove_rec(value: &mut Value, path: Peekable<PathIter>, prune: bool) -> Option<(Value, bool)> {
    match value {
        Value::Map(map) => remove_map(map, path, prune),
        Value::Array(map) => remove_array(map, path, prune),
        _ => None,
    }
}

fn remove_array(
    array: &mut Vec<Value>,
    mut path: Peekable<PathIter>,
    prune: bool,
) -> Option<(Value, bool)> {
    match path.next()? {
        PathComponent::Index(index) => match path.peek() {
            None => array_remove(array, index).map(|v| (v, array.is_empty())),
            Some(_) => array
                .get_mut(index)
                .and_then(|value| remove_rec(value, path, prune)),
        },
        _ => None,
    }
}

fn remove_map(
    fields: &mut BTreeMap<String, Value>,
    mut path: Peekable<PathIter>,
    prune: bool,
) -> Option<(Value, bool)> {
    match path.next()? {
        PathComponent::Key(key) => match path.peek() {
            None => fields.remove(key.as_ref()).map(|v| (v, fields.is_empty())),
            Some(_) => {
                let (result, empty) = fields
                    .get_mut(key.as_ref())
                    .and_then(|value| remove_rec(value, path, prune))?;
                if prune && empty {
                    fields.remove(key.as_ref());
                }
                Some((result, fields.is_empty()))
            }
        },
        _ => None,
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
    use serde_json::json;

    use super::{super::test::fields_from_json, *};

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
        assert_eq!(
            remove(&mut fields, "field", false),
            Some(Value::Integer(123))
        );
        assert_eq!(remove(&mut fields, "field", false), None);
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

        for (query, expected_first, expected_second) in &queries {
            assert_eq!(
                remove(&mut fields, query, false),
                *expected_first,
                "{}",
                query
            );
            assert_eq!(
                remove(&mut fields, query, false),
                *expected_second,
                "{}",
                query
            );
        }
        assert_eq!(
            fields,
            fields_from_json(json!({
                "a": {
                    "b": {},
                    "array": [
                        null,
                        null,
                        {},
                        [],
                    ],
                },
            }))
        );
    }

    #[test]
    fn remove_prune() {
        let mut fields = fields_from_json(json!({
            "a": {
                "b": {
                    "c": vec![5]
                },
                "d": 4,
            }
        }));

        assert_eq!(remove(&mut fields, "a.d", true), Some(Value::Integer(4)));
        assert_eq!(
            fields,
            fields_from_json(json!({
                "a": {
                    "b": {
                        "c": vec![5]
                    }
                }
            }))
        );

        assert_eq!(
            remove(&mut fields, "a.b.c[0]", true),
            Some(Value::Integer(5))
        );
        assert_eq!(fields, fields_from_json(json!({})));
    }
}
