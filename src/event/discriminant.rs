use super::{LogEvent, Value};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
};
use string_cache::DefaultAtom as Atom;

// TODO: if we had `Value` implement `Eq` and `Hash`, the implementation here
// would be much easier. The issue is with `f64` type. We should consider using
// a newtype for `f64` there that'd implement `Eq` and `Hash` is it's safe, for
// example `NormalF64`, and guard the values with `val.is_normal() == true`
// invariant.
// See also: https://internals.rust-lang.org/t/f32-f64-should-implement-hash/5436/32

/// An event discriminant identifies a distinguishable subset of events.
/// Intended for disecting streams of events to substreams, for instance to
/// be able to allocate a buffer per substream.
/// Implements `PartialEq`, `Eq` and `Hash` to enable use as a `HashMap` key.
#[derive(Debug)]
pub struct Discriminant {
    values: Vec<Option<Value>>,
}

impl Discriminant {
    /// Create a new Discriminant from the `LogEvent` and an ordered slice of
    /// fields to include into a discriminant value.
    pub fn from_log_event(event: &LogEvent, discriminant_fields: &[Atom]) -> Self {
        let values: Vec<Option<Value>> = discriminant_fields
            .iter()
            .map(|discriminant_field| event.get(discriminant_field).cloned())
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

// Equality check for for discriminant purposes.
fn value_eq(this: &Value, other: &Value) -> bool {
    match (this, other) {
        // Trivial.
        (Value::Bytes(this), Value::Bytes(other)) => this.eq(other),
        (Value::Boolean(this), Value::Boolean(other)) => this.eq(other),
        (Value::Integer(this), Value::Integer(other)) => this.eq(other),
        (Value::Timestamp(this), Value::Timestamp(other)) => this.eq(other),
        // Non-trivial.
        (Value::Float(this), Value::Float(other)) => f64_eq(this, other),
        // Type mismatch.
        _ => false,
    }
}

// Does an f64 comparison that is suitable for discriminant purposes.
fn f64_eq(this: &f64, other: &f64) -> bool {
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
        Value::Boolean(val) => val.hash(hasher),
        Value::Integer(val) => val.hash(hasher),
        Value::Timestamp(val) => val.hash(hasher),
        // Non-trivial.
        Value::Float(val) => hash_f64(hasher, val),
        Value::Array(val) => hash_array(hasher, val),
        Value::Map(val) => hash_map(hasher, val),
    }
}

// Does f64 hashing that is suitable for discriminant purposes.
fn hash_f64<H: Hasher>(hasher: &mut H, value: &f64) {
    hasher.write(&value.to_ne_bytes());
}

fn hash_array<H: Hasher>(hasher: &mut H, array: &Vec<Value>) {
    for val in array.iter() {
        hash_value(hasher, val);
    }
}

fn hash_map<H: Hasher>(hasher: &mut H, map: &HashMap<Atom, Value>) {
    // FIXME: until we replaced HashMap by a more suitable map collection this might be broken
    for (key, val) in map.iter() {
        hasher.write(key.as_bytes());
        hash_value(hasher, val);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use std::collections::{hash_map::DefaultHasher, HashMap};

    fn new_log_event() -> LogEvent {
        Event::new_empty_log().into_log()
    }

    fn hash<H: Hash>(hash: H) -> u64 {
        let mut hasher = DefaultHasher::new();
        hash.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equal() {
        let mut event_1 = new_log_event();
        event_1.insert("hostname", "localhost");
        event_1.insert("irrelevant", "not even used");
        let mut event_2 = event_1.clone();
        event_2.insert("irrelevant", "does not matter if it's different");

        let discriminant_fields = vec![Atom::from("hostname"), Atom::from("container_id")];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn not_equal() {
        let mut event_1 = new_log_event();
        event_1.insert("hostname", "localhost");
        event_1.insert("container_id", "abc");
        let mut event_2 = event_1.clone();
        event_2.insert("container_id", "def");

        let discriminant_fields = vec![Atom::from("hostname"), Atom::from("container_id")];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_ne!(discriminant_1, discriminant_2);
        assert_ne!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn field_order() {
        let mut event_1 = new_log_event();
        event_1.insert("a", "a");
        event_1.insert("b", "b");
        let mut event_2 = new_log_event();
        event_2.insert("b", "b");
        event_2.insert("a", "a");

        let discriminant_fields = vec![Atom::from("a"), Atom::from("b")];

        let discriminant_1 = Discriminant::from_log_event(&event_1, &discriminant_fields);
        let discriminant_2 = Discriminant::from_log_event(&event_2, &discriminant_fields);

        assert_eq!(discriminant_1, discriminant_2);
        assert_eq!(hash(discriminant_1), hash(discriminant_2));
    }

    #[test]
    fn with_hash_map() {
        let mut map: HashMap<Discriminant, usize> = HashMap::new();

        let event_stream_1 = {
            let mut event = new_log_event();
            event.insert("hostname", "a.test");
            event.insert("container_id", "abc");
            event
        };

        let event_stream_2 = {
            let mut event = new_log_event();
            event.insert("hostname", "b.test");
            event.insert("container_id", "def");
            event
        };

        let event_stream_3 = {
            let event = new_log_event();
            // no `hostname` or `container_id`
            event
        };

        let discriminant_fields = vec![Atom::from("hostname"), Atom::from("container_id")];

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

        // Now assert the amount of events processed per descriminant.
        assert_eq!(process_event(event_stream_1), 2);
        assert_eq!(process_event(event_stream_2), 2);
        assert_eq!(process_event(event_stream_3), 2);
    }
}
