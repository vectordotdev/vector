use super::{Atom, PathComponent, PathIter, Value};
use std::{cmp::Ordering, collections::BTreeMap, iter::Peekable, mem};

/// Removes field value specified by the given path and return its value.
///
/// A special case worth mentioning: if there is a nested array and an item is removed
/// from the middle of this array, then it is just replaced by `Value::Null`.
pub fn remove(fields: &mut BTreeMap<Atom, Value>, path: &str) -> Option<Value> {
    let mut path_iter = PathIter::new(path).peekable();

    let key = match path_iter.next() {
        Some(PathComponent::Key(key)) => key,
        _ => return None,
    };

    match path_iter.peek() {
        None => fields.remove(&key),
        Some(_) => match fields.get_mut(&key) {
            None => None,
            Some(value) => value_remove(value, path_iter),
        },
    }
}

fn value_remove<I>(mut value: &mut Value, mut path_iter: Peekable<I>) -> Option<Value>
where
    I: Iterator<Item = PathComponent>,
{
    loop {
        value = match (path_iter.next(), value) {
            (Some(PathComponent::Key(ref key)), Value::Map(map)) => match path_iter.peek() {
                None => return map.remove(key),
                Some(_) => match map.get_mut(key) {
                    None => return None,
                    Some(value) => value,
                },
            },
            (Some(PathComponent::Index(index)), Value::Array(array)) => match path_iter.peek() {
                None => return array_remove(array, index),
                Some(_) => match array.get_mut(index) {
                    None => return None,
                    Some(value) => value,
                },
            },
            _ => return None,
        }
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
