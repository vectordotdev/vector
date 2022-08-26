use lookup::path;
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use vector_common::byte_size_of::JsonEncodedSizeOf;

use super::*;
use crate::event::test::common::Name;

const COLON_SIZE: usize = 1;
const COMMA_SIZE: usize = 1;

/// The action that our model interpreter loop will take.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    SizeOf,
    InsertFlat { key: String, value: Value },
    Remove { key: String },
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 2 {
            0 => Action::InsertFlat {
                key: String::from(Name::arbitrary(g)),
                value: Value::arbitrary(g),
            },
            1 => Action::SizeOf,
            3 => Action::Remove {
                key: String::from(Name::arbitrary(g)),
            },
            _ => unreachable!(),
        }
    }
}

#[test]
#[allow(clippy::print_stderr)]
fn log_operation_maintains_size() {
    // Asserts that the stated size of a LogEvent only changes by the amount
    // that we insert / remove from it and that read-only operations do not
    // change the size.
    fn inner(actions: Vec<Action>, mut log_event: LogEvent) -> TestResult {
        let mut current_size = log_event.json_encoded_size_of();
        let start_event = log_event.clone();
        let mut processed_actions = vec![];

        let mut remove_trailing_comma = false;

        for action in actions {
            processed_actions.push(action.clone());

            match action {
                Action::InsertFlat { key, value } => {
                    let new_value_sz = value.json_encoded_size_of();
                    let old_value_sz = log_event
                        .get(path!(key.as_str()))
                        .map_or(0, JsonEncodedSizeOf::json_encoded_size_of);

                    if !log_event.contains(key.as_str()) {
                        remove_trailing_comma = true;

                        current_size += key.json_encoded_size_of() + COLON_SIZE + COMMA_SIZE;
                    }

                    log_event.insert(path!(&key), value);

                    current_size -= old_value_sz;
                    current_size += new_value_sz;
                }
                Action::SizeOf => {
                    // If the event is empty at the start, and we added new fields, we need to
                    // remove the trailing comma.
                    if start_event.is_empty_object() && remove_trailing_comma {
                        current_size -= COMMA_SIZE;
                    }

                    if current_size != log_event.json_encoded_size_of() {
                        eprintln!("----------------------------");
                        eprintln!(
                            "start event ({}): {}",
                            start_event.json_encoded_size_of(),
                            start_event.value()
                        );
                        eprintln!(
                            "final event ({}): {}",
                            log_event.json_encoded_size_of(),
                            log_event.value()
                        );
                        eprintln!("processed actions: {:#?}", processed_actions);
                        assert_eq!(current_size, log_event.json_encoded_size_of(),);
                        eprintln!("----------------------------");
                    }

                    if start_event.is_empty_object() && remove_trailing_comma {
                        current_size += COMMA_SIZE;
                    }
                }
                Action::Remove { key } => {
                    let value_sz = log_event.remove(key.as_str()).json_encoded_size_of();
                    current_size -= value_sz;
                    current_size -= key.json_encoded_size_of();
                }
            }
        }

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Vec<Action>, LogEvent) -> TestResult);
}
