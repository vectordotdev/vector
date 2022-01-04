use std::collections::BTreeMap;

use super::{PathComponent, PathIter, Value};

/// Returns a mutable reference to field value specified by the given path.
pub fn get_mut<'a>(fields: &'a mut BTreeMap<String, Value>, path: &str) -> Option<&'a mut Value> {
    let mut path_iter = PathIter::new(path);

    match path_iter.next() {
        Some(PathComponent::Key(key)) => match fields.get_mut(key.as_ref()) {
            None => None,
            Some(value) => get_mut_value(value, path_iter),
        },
        _ => None,
    }
}

fn get_mut_value<'a, I>(mut value: &mut Value, mut path_iter: I) -> Option<&mut Value>
where
    I: Iterator<Item = PathComponent<'a>>,
{
    loop {
        match (path_iter.next(), value) {
            (None, value) => return Some(value),
            (Some(PathComponent::Key(key)), Value::Map(map)) => match map.get_mut(key.as_ref()) {
                None => return None,
                Some(nested_value) => {
                    value = nested_value;
                }
            },
            (Some(PathComponent::Index(index)), Value::Array(array)) => {
                match array.get_mut(index) {
                    None => return None,
                    Some(nested_value) => {
                        value = nested_value;
                    }
                }
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::{super::test::fields_from_json, *};

    #[test]
    fn get_mut_simple() {
        let mut fields = fields_from_json(json!({
            "field": 123
        }));
        assert_eq!(
            get_mut(&mut fields, "field"),
            Some(&mut Value::Integer(123))
        );
    }

    #[test]
    fn get_mut_nested() {
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
        let mut queries = [
            ("a.b.c", Some(Value::Integer(5))),
            ("a.d", Some(Value::Integer(4))),
            ("a.array[1]", Some(Value::Integer(3))),
            ("a.array[2].x", Some(Value::Integer(1))),
            ("a.array[3][0]", Some(Value::Integer(2))),
            ("a.array[3][1]", None),
            ("a.x", None),
            ("z", None),
            (".123", None),
            ("", None),
        ];

        for (query, expected) in &mut queries {
            assert_eq!(get_mut(&mut fields, query), expected.as_mut(), "{}", query);
        }
    }
}
