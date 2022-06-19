use crate::value::crud::ValueCollection;
use crate::Value;
use lookup::lookup_v2::BorrowedSegment;

pub fn get_mut<'a>(
    mut value: &mut Value,
    mut path_iter: impl Iterator<Item = BorrowedSegment<'a>>,
) -> Option<&mut Value> {
    loop {
        match (path_iter.next(), value) {
            (None, value) => return Some(value),
            (Some(BorrowedSegment::Field(key)), Value::Object(map)) => {
                match map.get_mut_value(key.as_ref()) {
                    None => return None,
                    Some(nested_value) => {
                        value = nested_value;
                    }
                }
            }
            (Some(BorrowedSegment::Index(index)), Value::Array(array)) => {
                match array.get_mut_value(&index) {
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
