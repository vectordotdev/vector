use crate::event::{LogEvent, Value};
use std::collections::HashMap;
use string_cache::DefaultAtom as Atom;

/// Merges all fields specified at `merge_fields` from `incoming` to `current`.
pub fn merge_log_event(current: &mut LogEvent, mut incoming: LogEvent, merge_fields: &[Atom]) {
    for merge_field in merge_fields {
        let incoming_val = match incoming.remove(merge_field) {
            None => continue,
            Some(val) => val,
        };
        match current.get_mut(merge_field) {
            None => current.insert(merge_field, incoming_val),
            Some(current_val) => merge_value(current_val, incoming_val),
        }
    }
}

/// Merges `incoming` value into `current` value.
///
/// Will concatenate `Bytes` and overwrite the rest value kinds.
pub fn merge_value(current: &mut Value, incoming: Value) {
    match (current, incoming) {
        (Value::Bytes(current), Value::Bytes(ref incoming)) => current.extend_from_slice(incoming),
        (current, incoming) => *current = incoming,
    }
}

/// Merges `source` map into `dest` recursively
pub fn merge_nested(dest: &mut HashMap<Atom, Value>, source: HashMap<Atom, Value>) {
    for (key, value) in source.into_iter() {
        if let Some(Value::Map(dest_map)) = dest.get_mut(&key) {
            if let Value::Map(source_map) = value {
                merge_nested(dest_map, source_map);
            } else {
                dest.insert(key, value);
            }
        } else {
            dest.insert(key, value);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::Event;
    use std::collections::HashMap;

    fn assert_merge_value(
        current: impl Into<Value>,
        incoming: impl Into<Value>,
        expected: impl Into<Value>,
    ) {
        let mut merged = current.into();
        merge_value(&mut merged, incoming.into());
        assert_eq!(merged, expected.into());
    }

    #[test]
    fn merge_value_works_correctly() {
        assert_merge_value("hello ", "world", "hello world");

        assert_merge_value(true, false, false);
        assert_merge_value(false, true, true);

        assert_merge_value("my_val", true, true);
        assert_merge_value(true, "my_val", "my_val");

        assert_merge_value(1, 2, 2);
    }

    fn new_log_event() -> LogEvent {
        Event::new_empty_log().into_log()
    }

    #[test]
    fn merge_event_combines_values_accordingly() {
        // Specify the fields that will be merged.
        // Only the ones listed will be merged from the `incoming` event
        // to the `current`.
        let fields_to_merge = [
            Atom::from("merge"),
            Atom::from("merge_a"),
            Atom::from("merge_b"),
            Atom::from("merge_c"),
        ];

        let current = {
            let mut log = new_log_event();

            log.insert("merge", "hello "); // will be concatenated with the `merged` from `incoming`.
            log.insert("do_not_merge", "my_first_value"); // will remain as is, since it's not selected for merging.

            log.insert("merge_a", true); // will be overwritten with the `merge_a` from `incoming` (since it's a non-bytes kind).
            log.insert("merge_b", 123); // will be overwritten with the `merge_b` from `incoming` (since it's a non-bytes kind).

            log.insert("a", true); // will remain as is since it's not selected for merge.
            log.insert("b", 123); // will remain as is since it's not selected for merge.

            // `c` is not present in the `current`, and not selected for merge,
            // so it won't be included in the final event.

            log
        };

        let incoming = {
            let mut log = new_log_event();

            log.insert("merge", "world"); // will be concatenated to the `merge` from `current`.
            log.insert("do_not_merge", "my_second_value"); // will be ignored, since it's not selected for merge.

            log.insert("merge_b", 456); // will be merged in as `456`.
            log.insert("merge_c", false); // will be merged in as `false`.

            // `a` will remain as is, since it's not marked for merge and
            // niether it is specified in the `incoming` event.
            log.insert("b", 456); // `b` not marked for merge, will not change.
            log.insert("c", true); // `c` not marked for merge, will be ignored.

            log
        };

        let mut merged = current.clone();
        merge_log_event(&mut merged, incoming, &fields_to_merge);

        let expected = {
            let mut log = new_log_event();
            log.insert("merge", "hello world");
            log.insert("do_not_merge", "my_first_value");
            log.insert("a", true);
            log.insert("b", 123);
            log.insert("merge_a", true);
            log.insert("merge_b", 456);
            log.insert("merge_c", false);
            log
        };

        assert_eq!(merged, expected);
    }

    #[test]
    fn merge_nested_override() {
        let mut dest = HashMap::new();
        dest.insert(Atom::from("a"), Value::from("b"));
        dest.insert(Atom::from("c"), Value::from("d"));
        let mut nested = HashMap::new();
        nested.insert(Atom::from("x1"), Value::from("y1"));
        nested.insert(Atom::from("x2"), Value::from("y2"));
        dest.insert(Atom::from("nested"), Value::Map(nested));

        let mut source = HashMap::new();
        source.insert(Atom::from("a"), Value::from("c"));
        source.insert(Atom::from("e"), Value::from("f"));
        let mut nested = HashMap::new();
        nested.insert(Atom::from("x1"), Value::from("y3"));
        nested.insert(Atom::from("w"), Value::from("u"));
        dest.insert(Atom::from("nested"), Value::Map(nested));

        merge_nested(&mut dest, source);
        assert_eq!(dest.get(&Atom::from("a")), Some(&Value::from("c")));
        assert_eq!(dest.get(&Atom::from("c")), Some(&Value::from("d")));
        assert_eq!(dest.get(&Atom::from("e")), Some(&Value::from("f")));
        if let Some(Value::Map(nested)) = dest.get(&Atom::from("nested")) {
            assert_eq!(nested.get(&Atom::from("x1")), Some(&Value::from("y3")));
            assert_eq!(nested.get(&Atom::from("w")), Some(&Value::from("u")));
        } else {
            unreachable!("The \"nested\" field is not present");
        }
    }
}
