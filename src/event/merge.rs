use crate::event::{LogEvent, ValueKind};
use string_cache::DefaultAtom as Atom;

/// Merges all fields specified at `merge_fields` from `incoming` to `current`.
pub fn merge_log_event(current: &mut LogEvent, mut incoming: LogEvent, merge_fields: &[Atom]) {
    for merge_field in merge_fields {
        let (incoming_val, is_explicit) = match incoming.remove_with_explicitness(merge_field) {
            None => continue,
            Some(val) => val,
        };
        match current.get_mut(merge_field) {
            None => {
                // TODO: here we do tricks to properly propagate the
                // explcitness status of the value. This should be simplified to
                // just a plain `insert` of the value once when we get rid of
                // the `explicit` bool in the `Value` and the legacy
                // explicitness notion.
                if is_explicit {
                    current.insert_explicit(merge_field, incoming_val)
                } else {
                    current.insert_implicit(merge_field, incoming_val)
                }
            }
            Some(current_val) => merge_value(current_val, incoming_val),
        }
    }
}

pub fn merge_value(current: &mut ValueKind, incoming: ValueKind) {
    match incoming {
        ValueKind::Bytes(incoming_bytes) => match current {
            ValueKind::Bytes(current_bytes) => {
                current_bytes.extend_from_slice(incoming_bytes.as_ref())
            }
            other_current => *other_current = ValueKind::Bytes(incoming_bytes),
        },
        other => *current = other,
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::event::Event;

    fn assert_merge_value(
        current: impl Into<ValueKind>,
        incoming: impl Into<ValueKind>,
        expected: impl Into<ValueKind>,
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

            log.insert_implicit("merge", "hello "); // will be concatenated with the `merged` from `incoming`.
            log.insert_implicit("do_not_merge", "my_first_value"); // will remain as is, since it's not selected for merging.

            log.insert_implicit("merge_a", true); // will be overwritten with the `merge_a` from `incoming` (since it's a non-bytes kind).
            log.insert_implicit("merge_b", 123); // will be overwritten with the `merge_b` from `incoming` (since it's a non-bytes kind).

            log.insert_implicit("a", true); // will remain as is since it's not selected for merge.
            log.insert_implicit("b", 123); // will remain as is since it's not selected for merge.

            // `c` is not present in the `current`, and not selected for merge,
            // so it won't be included in the final event.

            log
        };

        let incoming = {
            let mut log = new_log_event();

            log.insert_implicit("merge", "world"); // will be concatenated to the `merge` from `current`.
            log.insert_implicit("do_not_merge", "my_second_value"); // will be ignored, since it's not selected for merge.

            log.insert_implicit("merge_b", 456); // will be merged in as `456`.
            log.insert_implicit("merge_c", false); // will be merged in as `false`.

            // `a` will remain as is, since it's not marked for merge and
            // niether it is specified in the `incoming` event.
            log.insert_implicit("b", 456); // `b` not marked for merge, will not change.
            log.insert_implicit("c", true); // `c` not marked for merge, will be ignored.

            log
        };

        let mut merged = current.clone();
        merge_log_event(&mut merged, incoming, &fields_to_merge);

        let expected = {
            let mut log = new_log_event();
            log.insert_implicit("merge", "hello world");
            log.insert_implicit("do_not_merge", "my_first_value");
            log.insert_implicit("a", true);
            log.insert_implicit("b", 123);
            log.insert_implicit("merge_a", true);
            log.insert_implicit("merge_b", 456);
            log.insert_implicit("merge_c", false);
            log
        };

        assert_eq!(merged, expected);
    }
}
