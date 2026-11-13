use std::mem;

use lookup::{PathPrefix, path};
use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
use vector_common::byte_size_of::ByteSizeOf;
use vrl::path::{OwnedSegment, OwnedValuePath};

use super::*;
use crate::event::arbitrary_impl::Name;

#[test]
fn at_least_wrapper_size() {
    // The byte size of an `Event` should always be at least as big as the
    // mem::size_of of the `Event`.
    #[allow(clippy::needless_pass_by_value)]
    fn inner(event: Event) -> TestResult {
        let baseline = mem::size_of::<Event>();
        assert!(baseline <= event.size_of());
        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Event) -> TestResult);
}

#[test]
fn exactly_equal_if_no_allocated_bytes() {
    // The byte size of an `Event` should always be exactly equal to its
    // `mem::size_of` if there are no reported allocated bytes.
    #[allow(clippy::needless_pass_by_value)]
    fn inner(event: Event) -> TestResult {
        let allocated_sz = event.allocated_bytes();
        if allocated_sz == 0 {
            let baseline = mem::size_of::<Event>();
            assert_eq!(baseline, event.size_of());
            return TestResult::passed();
        }
        TestResult::discard()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Event) -> TestResult);
}

#[test]
fn size_greater_than_allocated_size() {
    // The total byte size of an `Event` should always be strictly greater than
    // the allocated bytes of the `Event`.
    #[allow(clippy::needless_pass_by_value)]
    fn inner(event: Event) -> TestResult {
        let total_sz = event.size_of();
        let allocated_sz = event.allocated_bytes();

        assert!(total_sz > allocated_sz);
        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Event) -> TestResult);
}

//
// Log Events
//

/// The action that our model interpreter loop will take.
#[derive(Debug, Clone)]
pub(crate) enum Action {
    Contains {
        key: KeyString,
    },
    SizeOf,
    /// Insert a key/value pair into the [`LogEvent`]
    InsertFlat {
        key: KeyString,
        value: Value,
    },
    Remove {
        key: KeyString,
    },
}

impl Arbitrary for Action {
    fn arbitrary(g: &mut Gen) -> Self {
        match u8::arbitrary(g) % 3 {
            0 => Action::InsertFlat {
                key: String::from(Name::arbitrary(g)).into(),
                value: Value::arbitrary(g),
            },
            1 => Action::SizeOf,
            2 => Action::Contains {
                key: String::from(Name::arbitrary(g)).into(),
            },
            3 => Action::Remove {
                key: String::from(Name::arbitrary(g)).into(),
            },
            _ => unreachable!(),
        }
    }
}

#[test]
fn log_operation_maintains_size() {
    // Asserts that the stated size of a LogEvent only changes by the amount
    // that we insert / remove from it and that read-only operations do not
    // change the size.
    fn inner(actions: Vec<Action>, mut log_event: LogEvent) -> TestResult {
        let mut current_size = log_event.size_of();

        for action in actions {
            match action {
                Action::InsertFlat { key, value } => {
                    let new_value_sz = value.size_of();
                    let target_path = (PathPrefix::Event, path!(key.as_str()));
                    let old_value_sz = log_event.get(target_path).map_or(0, ByteSizeOf::size_of);
                    if !log_event.contains((PathPrefix::Event, path!(key.as_str()))) {
                        current_size += key.size_of();
                    }
                    log_event.insert(target_path, value);
                    current_size -= old_value_sz;
                    current_size += new_value_sz;
                }
                Action::SizeOf => {
                    assert_eq!(current_size, log_event.size_of());
                }
                Action::Contains { key } => {
                    log_event.contains((PathPrefix::Event, path!(key.as_str())));
                }
                Action::Remove { key } => {
                    let value_sz = log_event
                        .remove((PathPrefix::Event, path!(key.as_str())))
                        .size_of();
                    current_size -= value_sz;
                    current_size -= key.size_of();
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

/// Insertion, or removal with optional pruning when `value` is `None`, at a nested path drawn from a small set of keys
/// and indices so that actions often hit existing fields.
#[derive(Debug, Clone)]
struct JsonSizeAction {
    path: OwnedValuePath,
    value: Option<Value>,
    prune: bool,
    /// Keeps a clone of the event alive during the action, so that the mutation copies the
    /// shared inner value.
    shared: bool,
}

impl Arbitrary for JsonSizeAction {
    fn arbitrary(g: &mut Gen) -> Self {
        let depth = 1 + usize::arbitrary(g) % 3;
        let segments = (0..depth)
            .map(|_| match u8::arbitrary(g) % 5 {
                0 => OwnedSegment::Index(isize::from(i8::arbitrary(g) % 3)),
                n => OwnedSegment::Field(["a", "b", "c", "d"][usize::from(n - 1)].into()),
            })
            .collect::<Vec<_>>();
        Self {
            path: segments.into(),
            value: bool::arbitrary(g).then(|| Value::arbitrary(g)),
            prune: bool::arbitrary(g),
            shared: bool::arbitrary(g),
        }
    }
}

#[test]
fn log_operation_maintains_estimated_json_size() {
    // Asserts that the cached estimated JSON size of a LogEvent always matches a full recompute
    // after insertions and removals.
    fn inner(actions: Vec<JsonSizeAction>, mut log_event: LogEvent) -> TestResult {
        for action in actions {
            log_event.estimated_json_encoded_size_of();
            let shared = action.shared.then(|| log_event.clone());
            let path = (PathPrefix::Event, &action.path);
            match action.value {
                Some(value) => _ = log_event.insert(path, value),
                None => _ = log_event.remove_prune(path, action.prune),
            }
            drop(shared);

            assert_eq!(
                log_event.estimated_json_encoded_size_of(),
                log_event.value().estimated_json_encoded_size_of(),
                "{}",
                action.path
            );
        }

        TestResult::passed()
    }

    QuickCheck::new()
        .tests(1_000)
        .max_tests(10_000)
        .quickcheck(inner as fn(Vec<JsonSizeAction>, LogEvent) -> TestResult);
}
