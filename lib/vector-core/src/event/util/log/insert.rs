use lookup::lookup_v2::{BorrowedSegment, Path};
use std::{collections::BTreeMap, iter::Peekable};

use super::Value;

/// Inserts field value using a path specified using `a.b[1].c` notation.
#[allow(clippy::needless_pass_by_value)] // impl Path is always a reference
pub fn insert<'a>(
    fields: &mut BTreeMap<String, Value>,
    path: impl Path<'a>,
    value: Value,
) -> Option<Value> {
    let path_iter = path.segment_iter().peekable();
    map_insert(fields, path_iter, value)
}

fn map_insert<'a>(
    fields: &mut BTreeMap<String, Value>,
    mut path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    value: Value,
) -> Option<Value> {
    match (path_iter.next(), path_iter.peek()) {
        (Some(BorrowedSegment::Field(current)), None) => fields.insert(current.to_string(), value),
        (Some(BorrowedSegment::Field(current)), Some(BorrowedSegment::Field(_))) => {
            if let Some(Value::Object(map)) = fields.get_mut(current.as_ref()) {
                map_insert(map, path_iter, value)
            } else {
                let mut map = BTreeMap::new();
                map_insert(&mut map, path_iter, value);
                fields.insert(current.to_string(), Value::Object(map))
            }
        }
        (Some(BorrowedSegment::Field(current)), Some(&BorrowedSegment::Index(next))) => {
            if let Some(Value::Array(array)) = fields.get_mut(current.as_ref()) {
                array_insert(array, path_iter, value)
            } else {
                let mut array = Vec::with_capacity((next as usize) + 1);
                array_insert(&mut array, path_iter, value);
                fields.insert(current.to_string(), Value::Array(array))
            }
        }
        _ => None,
    }
}

fn array_insert<'a>(
    values: &mut Vec<Value>,
    mut path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    value: Value,
) -> Option<Value> {
    match (path_iter.next(), path_iter.peek()) {
        (Some(BorrowedSegment::Index(current)), None) => {
            while values.len() <= (current as usize) {
                values.push(Value::Null);
            }
            Some(std::mem::replace(&mut values[current as usize], value))
        }
        (Some(BorrowedSegment::Index(current)), Some(BorrowedSegment::Field(_))) => {
            if let Some(Value::Object(map)) = values.get_mut(current as usize) {
                map_insert(map, path_iter, value)
            } else {
                let mut map = BTreeMap::new();
                map_insert(&mut map, path_iter, value);
                while values.len() <= (current as usize) {
                    values.push(Value::Null);
                }
                Some(std::mem::replace(
                    &mut values[current as usize],
                    Value::Object(map),
                ))
            }
        }
        (Some(BorrowedSegment::Index(current)), Some(BorrowedSegment::Index(next))) => {
            if let Some(Value::Array(array)) = values.get_mut(current as usize) {
                array_insert(array, path_iter, value)
            } else {
                let mut array = Vec::with_capacity((*next as usize) + 1);
                array_insert(&mut array, path_iter, value);
                while values.len() <= (current as usize) {
                    values.push(Value::Null);
                }
                Some(std::mem::replace(
                    &mut values[current as usize],
                    Value::Array(array),
                ))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use serde_json::json;

    use super::{super::test::fields_from_json, *};

    #[test]
    fn test_insert_nested() {
        let mut fields = BTreeMap::new();
        insert(&mut fields, "a.b.c", Value::Integer(3));

        let expected = fields_from_json(json!({
            "a": {
                "b":{
                    "c": 3
                }
            }
        }));
        assert_eq!(fields, expected);
    }

    #[test]
    fn test_insert_array() {
        let mut fields = BTreeMap::new();
        insert(&mut fields, "a.b[0].c[2]", Value::Integer(10));
        insert(&mut fields, "a.b[0].c[0]", Value::Integer(5));

        let expected = fields_from_json(json!({
            "a": {
                "b": [{
                    "c": [5, null, 10]
                }]
            }
        }));
        assert_eq!(fields, expected);
    }
}
