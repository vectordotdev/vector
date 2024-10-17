use std::hash::{Hash, Hasher};

use super::{LogEvent, ObjectMap, Value};

// TODO: if we had `Value` implement `Eq` and `Hash`, the implementation here
// would be much easier. The issue is with `f64` type. We should consider using
// a newtype for `f64` there that'd implement `Eq` and `Hash` if it's safe, for
// example `NormalF64`, and guard the values with `val.is_normal() == true`
// invariant.
// See also: https://internals.rust-lang.org/t/f32-f64-should-implement-hash/5436/32

/// An event discriminant identifies a distinguishable subset of events.
/// Intended for dissecting streams of events to sub-streams, for instance to
/// be able to allocate a buffer per sub-stream.
/// Implements `PartialEq`, `Eq` and `Hash` to enable use as a `HashMap` key.
#[derive(Debug, Clone)]
pub struct Discriminant {
    values: Vec<Option<Value>>,
}

impl Discriminant {
    /// Create a new Discriminant from the `LogEvent` and an ordered slice of
    /// fields to include into a discriminant value.
    pub fn from_log_event(event: &LogEvent, discriminant_fields: &[impl AsRef<str>]) -> Self {
        let values: Vec<Option<Value>> = discriminant_fields
            .iter()
            .map(|discriminant_field| {
                event
                    .parse_path_and_get_value(discriminant_field.as_ref())
                    .ok()
                    .flatten()
                    .cloned()
            })
            .collect();
        Self { values }
    }
}

impl PartialEq for Discriminant {
    fn eq(&self, other: &Self) -> bool {
        self.values
            .iter()
            .zip(other.values.iter())
            .all(|(this, other)| match (this, other) {
                (None, None) => true,
                (Some(this), Some(other)) => value_eq(this, other),
                _ => false,
            })
    }
}

impl Eq for Discriminant {}

// Equality check for discriminant purposes.
fn value_eq(this: &Value, other: &Value) -> bool {
    match (this, other) {
        // Trivial.
        (Value::Bytes(this), Value::Bytes(other)) => this.eq(other),
        (Value::Boolean(this), Value::Boolean(other)) => this.eq(other),
        (Value::Integer(this), Value::Integer(other)) => this.eq(other),
        (Value::Timestamp(this), Value::Timestamp(other)) => this.eq(other),
        (Value::Null, Value::Null) => true,
        // Non-trivial.
        (Value::Float(this), Value::Float(other)) => f64_eq(this.into_inner(), other.into_inner()),
        (Value::Array(this), Value::Array(other)) => array_eq(this, other),
        (Value::Object(this), Value::Object(other)) => map_eq(this, other),
        // Type mismatch.
        _ => false,
    }
}

// Does an f64 comparison that is suitable for discriminant purposes.
fn f64_eq(this: f64, other: f64) -> bool {
    if this.is_nan() && other.is_nan() {
        return true;
    }
    if this != other {
        return false;
    };
    if (this.is_sign_positive() && other.is_sign_negative())
        || (this.is_sign_negative() && other.is_sign_positive())
    {
        return false;
    }
    true
}

fn array_eq(this: &[Value], other: &[Value]) -> bool {
    if this.len() != other.len() {
        return false;
    }

    this.iter()
        .zip(other.iter())
        .all(|(first, second)| value_eq(first, second))
}

fn map_eq(this: &ObjectMap, other: &ObjectMap) -> bool {
    if this.len() != other.len() {
        return false;
    }

    this.iter()
        .zip(other.iter())
        .all(|((key1, value1), (key2, value2))| key1 == key2 && value_eq(value1, value2))
}

impl Hash for Discriminant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for value in &self.values {
            match value {
                Some(value) => {
                    state.write_u8(1);
                    hash_value(state, value);
                }
                None => state.write_u8(0),
            }
        }
    }
}

// Hashes value for discriminant purposes.
fn hash_value<H: Hasher>(hasher: &mut H, value: &Value) {
    match value {
        // Trivial.
        Value::Bytes(val) => val.hash(hasher),
        Value::Regex(val) => val.as_bytes_slice().hash(hasher),
        Value::Boolean(val) => val.hash(hasher),
        Value::Integer(val) => val.hash(hasher),
        Value::Timestamp(val) => val.hash(hasher),
        // Non-trivial.
        Value::Float(val) => hash_f64(hasher, val.into_inner()),
        Value::Array(val) => hash_array(hasher, val),
        Value::Object(val) => hash_map(hasher, val),
        Value::Null => hash_null(hasher),
    }
}

// Does f64 hashing that is suitable for discriminant purposes.
fn hash_f64<H: Hasher>(hasher: &mut H, value: f64) {
    hasher.write(&value.to_ne_bytes());
}

fn hash_array<H: Hasher>(hasher: &mut H, array: &[Value]) {
    for val in array {
        hash_value(hasher, val);
    }
}

fn hash_map<H: Hasher>(hasher: &mut H, map: &ObjectMap) {
    for (key, val) in map {
        hasher.write(key.as_bytes());
        hash_value(hasher, val);
    }
}

fn hash_null<H: Hasher>(hasher: &mut H) {
    hasher.write_u8(0);
}

#[cfg(test)]
mod tests {
    use std::collections::{hash_map::DefaultHasher, HashMap};

    use super::*;
    use crate::event::LogEvent;

    fn hash<H: Hash>(hash: H) -> u64 {
        let mut hasher = DefaultHasher::new();
        hash.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equal() {
        let mut event_1 = LogEvent::default();
        event_1.insert("hostname", "localhost");
        event_1.insert("irrelevant", "not even used");
        let mut event_2 = event_1.clone();
        event_2.insert("irrelevant", "does not matter if it's different");

        let discriminant_fields = vec!["hostname".to_string(), "container_id".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn not_equal() {
        let mut event_1 = LogEvent::default();
        event_1.insert("hostname", "localhost");
        event_1.insert("container_id", "abc");
        let mut event_2 = event_1.clone();
        event_2.insert("container_id", "def");

        let discriminant_fields = vec!["hostname".to_string(), "container_id".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_ne!(discriminant_1, discriminant_2);
        assert_ne!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn field_order() {
        let mut event_1 = LogEvent::default();
        event_1.insert("a", "a");
        event_1.insert("b", "b");
        let mut event_2 = LogEvent::default();
        event_2.insert("b", "b");
        event_2.insert("a", "a");

        let discriminant_fields = vec!["a".to_string(), "b".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn map_values_key_order() {
        let mut event_1 = LogEvent::default();
        event_1.insert("nested.a", "a");
        event_1.insert("nested.b", "b");
        let mut event_2 = LogEvent::default();
        event_2.insert("nested.b", "b");
        event_2.insert("nested.a", "a");

        let discriminant_fields = vec!["nested".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn array_values_insertion_order() {
        let mut event_1 = LogEvent::default();
        event_1.insert("array[0]", "a");
        event_1.insert("array[1]", "b");
        let mut event_2 = LogEvent::default();
        event_2.insert("array[1]", "b");
        event_2.insert("array[0]", "a");

        let discriminant_fields = vec!["array".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn map_values_matter_1() {
        let mut event_1 = LogEvent::default();
        event_1.insert("nested.a", "a"); // `nested` is a `Value::Map`
        let event_2 = LogEvent::default(); // empty event

        let discriminant_fields = vec!["nested".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_ne!(discriminant_1, discriminant_2);
        assert_ne!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn map_values_matter_2() {
        let mut event_1 = LogEvent::default();
        event_1.insert("nested.a", "a"); // `nested` is a `Value::Map`
        let mut event_2 = LogEvent::default();
        event_2.insert("nested", "x"); // `nested` is a `Value::String`

        let discriminant_fields = vec!["nested".to_string()];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_ne!(discriminant_1, discriminant_2);
        assert_ne!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn with_hash_map() {
        #[allow(clippy::mutable_key_type)]
        let mut map: HashMap<Discriminant, usize> = HashMap::new();

        let event_stream_1 = {
            let mut event = LogEvent::default();
            event.insert("hostname", "a.test");
            event.insert("container_id", "abc");
            event
        };

        let event_stream_2 = {
            let mut event = LogEvent::default();
            event.insert("hostname", "b.test");
            event.insert("container_id", "def");
            event
        };

        let event_stream_3 = {
            // no `hostname` or `container_id`
            LogEvent::default()
        };

        let discriminant_fields = vec!["hostname".to_string(), "container_id".to_string()];

        let mut process_event = |event| {
            let discriminant = Discriminant::from_log_event(&event, &discriminant_fields);
            *map.entry(discriminant).and_modify(|e| *e += 1).or_insert(0)
        };

        {
            let mut event = event_stream_1.clone();
            event.insert("message", "a");
            assert_eq!(process_event(event), 0);
        }

        {
            let mut event = event_stream_1.clone();
            event.insert("message", "b");
            event.insert("irrelevant", "c");
            assert_eq!(process_event(event), 1);
        }

        {
            let mut event = event_stream_2.clone();
            event.insert("message", "d");
            assert_eq!(process_event(event), 0);
        }

        {
            let mut event = event_stream_2.clone();
            event.insert("message", "e");
            event.insert("irrelevant", "d");
            assert_eq!(process_event(event), 1);
        }

        {
            let mut event = event_stream_3.clone();
            event.insert("message", "f");
            assert_eq!(process_event(event), 0);
        }

        {
            let mut event = event_stream_3.clone();
            event.insert("message", "g");
            event.insert("irrelevant", "d");
            assert_eq!(process_event(event), 1);
        }

        // Now assert the amount of events processed per discriminant.
        assert_eq!(process_event(event_stream_1), 2);
        assert_eq!(process_event(event_stream_2), 2);
        assert_eq!(process_event(event_stream_3), 2);
    }
}
