use std::{collections::BTreeMap, iter::Peekable};

use lookup::lookup_v2::BorrowedSegment;

use super::Value;

pub fn map_insert<'a>(
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

pub fn array_insert<'a>(
    values: &mut Vec<Value>,
    mut path_iter: Peekable<impl Iterator<Item = BorrowedSegment<'a>>>,
    value: Value,
) -> Option<Value> {
    match (path_iter.next(), path_iter.peek()) {
        (Some(BorrowedSegment::Index(current)), None) => set_array_index(values, current, value),
        (Some(BorrowedSegment::Index(current)), Some(BorrowedSegment::Field(_))) => {
            if let Some(Value::Object(map)) = values.get_mut(current as usize) {
                map_insert(map, path_iter, value)
            } else {
                let mut map = BTreeMap::new();
                map_insert(&mut map, path_iter, value);
                set_array_index(values, current, Value::Object(map))
            }
        }
        (Some(BorrowedSegment::Index(current)), Some(BorrowedSegment::Index(next))) => {
            if let Some(Value::Array(array)) = values.get_mut(current as usize) {
                array_insert(array, path_iter, value)
            } else {
                let mut array = Vec::with_capacity((*next as usize) + 1);
                array_insert(&mut array, path_iter, value);
                set_array_index(values, current, Value::Array(array))
            }
        }
        _ => None,
    }
}

fn set_array_index(values: &mut Vec<Value>, index: usize, insert_value: Value) -> Option<Value> {
    if values.len() <= (index as usize) {
        while values.len() <= (index as usize) {
            values.push(Value::Null);
        }
        values[index as usize] = insert_value;
        None
    } else {
        Some(std::mem::replace(&mut values[index as usize], insert_value))
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use lookup::lookup_v2::Path;
    use serde_json::json;

    use super::*;

    #[test]
    fn test_insert_nested() {
        let mut fields = BTreeMap::new();
        map_insert(
            &mut fields,
            "a.b.c".segment_iter().peekable(),
            Value::Integer(3),
        );

        let expected = Value::from(json!({
            "a": {
                "b":{
                    "c": 3
                }
            }
        }))
        .as_object()
        .unwrap()
        .clone();
        assert_eq!(fields, expected);
    }

    #[test]
    fn test_insert_array() {
        let mut fields = BTreeMap::new();
        map_insert(
            &mut fields,
            "a.b[0].c[2]".segment_iter().peekable(),
            Value::Integer(10),
        );
        map_insert(
            &mut fields,
            "a.b[0].c[0]".segment_iter().peekable(),
            Value::Integer(5),
        );

        let expected = Value::from(json!({
            "a": {
                "b": [{
                    "c": [5, null, 10]
                }]
            }
        }))
        .as_object()
        .unwrap()
        .clone();
        assert_eq!(fields, expected);
    }
}
