use lookup::lookup_v2::{BorrowedSegment, Path};
use std::{cmp::Ordering, collections::BTreeMap, iter::Peekable, mem};

use super::Value;

/// Removes field value specified by the given path and return its value.
///
/// A special case worth mentioning: if there is a nested array and an item is removed
/// from the middle of this array, then it is just replaced by `Value::Null`.
#[allow(clippy::needless_pass_by_value)] // impl Path is always a reference
pub fn remove<'a>(value: &mut Value, path: impl Path<'a>, prune: bool) -> Option<Value> {
    let path_iter = path.segment_iter().peekable();
    remove_rec(value, path_iter, prune).map(|(value, _)| value)
}

/// Recursively iterate through the path, and remove the last path
/// element. This is the top-level function which can remove from any
/// type of `Value`.
fn remove_rec<'a>(
    value: &mut Value,
    path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    prune: bool,
) -> Option<(Value, bool)> {
    match value {
        Value::Object(map) => remove_map(map, path_iter, prune),
        Value::Array(map) => remove_array(map, path_iter, prune),
        _ => None,
    }
}

fn remove_array<'a>(
    array: &mut Vec<Value>,
    mut path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    prune: bool,
) -> Option<(Value, bool)> {
    match path_iter.next()? {
        BorrowedSegment::Index(index) => match path_iter.peek() {
            None => array_remove(array, index as usize).map(|v| (v, array.is_empty())),
            Some(_) => array
                .get_mut(index as usize)
                .and_then(|value| remove_rec(value, path_iter, prune)),
        },
        _ => None,
    }
}

fn remove_map<'a>(
    fields: &mut BTreeMap<String, Value>,
    mut path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    prune: bool,
) -> Option<(Value, bool)> {
    match path_iter.next()? {
        BorrowedSegment::Field(key) => match path_iter.peek() {
            None => fields.remove(key.as_ref()).map(|v| (v, fields.is_empty())),
            Some(_) => {
                let (result, empty) = fields
                    .get_mut(key.as_ref())
                    .and_then(|value| remove_rec(value, path_iter, prune))?;
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

    use super::*;

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
        let mut fields = Value::from(json!({
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
        let mut fields = Value::from(json!({
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
                remove(&mut fields, *query, false),
                *expected_first,
                "{}",
                query
            );
            assert_eq!(
                remove(&mut fields, *query, false),
                *expected_second,
                "{}",
                query
            );
        }
        assert_eq!(
            fields,
            Value::from(json!({
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
        let mut fields = Value::from(json!({
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
            Value::from(json!({
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
        assert_eq!(fields, Value::from(json!({})));
    }
}
