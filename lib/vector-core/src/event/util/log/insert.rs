use std::{collections::BTreeMap, iter::Peekable};

use super::{PathComponent, PathIter, Value};

/// Inserts field value using a path specified using `a.b[1].c` notation.
pub fn insert(fields: &mut BTreeMap<String, Value>, path: &str, value: Value) -> Option<Value> {
    map_insert(fields, PathIter::new(path).peekable(), value)
}

pub fn insert_path(
    fields: &mut BTreeMap<String, Value>,
    path: Vec<PathComponent>,
    value: Value,
) -> Option<Value> {
    map_insert(fields, path.into_iter().peekable(), value)
}

fn map_insert<'a, I>(
    fields: &mut BTreeMap<String, Value>,
    mut path_iter: Peekable<I>,
    value: Value,
) -> Option<Value>
where
    I: Iterator<Item = PathComponent<'a>>,
{
    match (path_iter.next(), path_iter.peek()) {
        (Some(PathComponent::Key(current)), None) => fields.insert(current.into_owned(), value),
        (Some(PathComponent::Key(current)), Some(PathComponent::Key(_))) => {
            if let Some(Value::Map(map)) = fields.get_mut(current.as_ref()) {
                map_insert(map, path_iter, value)
            } else {
                let mut map = BTreeMap::new();
                map_insert(&mut map, path_iter, value);
                fields.insert(current.into_owned(), Value::Map(map))
            }
        }
        (Some(PathComponent::Key(current)), Some(&PathComponent::Index(next))) => {
            if let Some(Value::Array(array)) = fields.get_mut(current.as_ref()) {
                array_insert(array, path_iter, value)
            } else {
                let mut array = Vec::with_capacity(next + 1);
                array_insert(&mut array, path_iter, value);
                fields.insert(current.into_owned(), Value::Array(array))
            }
        }
        _ => None,
    }
}

fn array_insert<'a, I>(
    values: &mut Vec<Value>,
    mut path_iter: Peekable<I>,
    value: Value,
) -> Option<Value>
where
    I: Iterator<Item = PathComponent<'a>>,
{
    match (path_iter.next(), path_iter.peek()) {
        (Some(PathComponent::Index(current)), None) => {
            while values.len() <= current {
                values.push(Value::Null);
            }
            Some(std::mem::replace(&mut values[current], value))
        }
        (Some(PathComponent::Index(current)), Some(PathComponent::Key(_))) => {
            if let Some(Value::Map(map)) = values.get_mut(current) {
                map_insert(map, path_iter, value)
            } else {
                let mut map = BTreeMap::new();
                map_insert(&mut map, path_iter, value);
                while values.len() <= current {
                    values.push(Value::Null);
                }
                Some(std::mem::replace(&mut values[current], Value::Map(map)))
            }
        }
        (Some(PathComponent::Index(current)), Some(PathComponent::Index(next))) => {
            if let Some(Value::Array(array)) = values.get_mut(current) {
                array_insert(array, path_iter, value)
            } else {
                let mut array = Vec::with_capacity(next + 1);
                array_insert(&mut array, path_iter, value);
                while values.len() <= current {
                    values.push(Value::Null);
                }
                Some(std::mem::replace(&mut values[current], Value::Array(array)))
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
