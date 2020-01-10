use super::Transform;
use crate::runtime::TaskExecutor;
use crate::{
    event::{self, Event, ValueKind},
    topology::config::{DataType, TransformConfig, TransformDescription},
};
use serde::{Deserialize, Serialize};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MergeConfig {
    /// The field that indicates that the event is partial. A consequent stream
    /// of partial events along with the first non-partial event will be merged
    /// together.
    pub partial_event_marker: Atom,
    /// Fields to merge. The values of these fields will be merged into the
    /// first partial event. Fields not specified here will be ignored.
    /// Merging process takes the first buffered partial event, then loops over
    /// the rest of them and merges in the fields from each buffered partial
    /// event.
    /// Finally, the non-partial event fields are merged in, producing the
    /// resulting merged event.
    pub merge_fields: Vec<Atom>,
}

inventory::submit! {
    TransformDescription::new::<MergeConfig>("merge")
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            partial_event_marker: event::PARTIAL.clone(),
            merge_fields: vec![event::MESSAGE.clone()],
        }
    }
}

#[typetag::serde(name = "merge")]
impl TransformConfig for MergeConfig {
    fn build(&self, _exec: TaskExecutor) -> crate::Result<Box<dyn Transform>> {
        Ok(Box::new(Merge::from(self.clone())))
    }

    fn input_type(&self) -> DataType {
        DataType::Log
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }

    fn transform_type(&self) -> &'static str {
        "merge"
    }
}

#[derive(Debug)]
pub struct Merge {
    partial_event_marker: Atom,
    merge_fields: Vec<Atom>,
    partial_events: Vec<Event>,
}

impl From<MergeConfig> for Merge {
    fn from(config: MergeConfig) -> Self {
        Self {
            partial_event_marker: config.partial_event_marker,
            merge_fields: config.merge_fields,
            partial_events: Vec::new(),
        }
    }
}

impl Transform for Merge {
    fn transform(&mut self, mut event: Event) -> Option<Event> {
        // TODO: `lua` transform doesn't support assigning non-string values.
        // Normally we'd check for the field value to be `true`, and only then
        // consider event partial, but, to simplify the integration, for now we
        // only check for the field presence. We can switch this to check the
        // value to be `true` when the `lua` supports setting boolean fields
        // easily, as we expect users to rely on `lua` transform to implement
        // custom partial markers.

        // If current event as the partial indicator, cosider itt partial.
        // Remove the partial marker from the event and stash it.
        if event
            .as_mut_log()
            .remove(&self.partial_event_marker)
            .is_some()
        {
            self.partial_events.push(event);
            return None;
        }

        // Short circut to returning the event as is if there're no pending
        // events.
        if self.partial_events.is_empty() {
            return Some(event);
        }

        let merge_fields = self.merge_fields.as_slice();

        // Merge all partial events.
        let mut drain = self.partial_events.drain(..);

        // Take the first partial event. We know this won't fail cause we
        // checked that partial events list is not empty earlier.
        let mut merged_event = drain.next().unwrap();

        // Merge all partial events into the merge event.
        for partial_event in drain {
            merge_event(&mut merged_event, partial_event, merge_fields);
        }

        // Merge the current event last.
        merge_event(&mut merged_event, event, merge_fields);

        // Return the merged event.
        Some(merged_event)
    }
}

fn merge_event(into: &mut Event, from: Event, merge_fields: &[Atom]) {
    let mut incoming_log = from.into_log();
    for merge_field in merge_fields {
        let (incoming_val, is_explicit) = match incoming_log.remove_with_explicitness(merge_field) {
            None => continue,
            Some(val) => val,
        };
        match into.as_mut_log().get_mut(merge_field) {
            None => {
                // TODO: here we do tricks just properly propagate the
                // explcitness status of the value. This should be simplified to
                // just insrtion of the value once when we get rid of the
                // `explicit` bool in the `Value`.
                if is_explicit {
                    into.as_mut_log().insert_explicit(merge_field, incoming_val)
                } else {
                    into.as_mut_log().insert_implicit(merge_field, incoming_val)
                }
            }
            Some(current_val) => merge_value(current_val, incoming_val),
        }
    }
}

fn merge_value(current: &mut ValueKind, incoming: ValueKind) {
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
    use super::{merge_event, merge_value, Merge, MergeConfig};
    use crate::event::{self, Event, ValueKind};
    use crate::transforms::Transform;
    use string_cache::DefaultAtom as Atom;

    fn make_partial(mut event: Event) -> Event {
        event
            .as_mut_log()
            .insert_explicit(event::PARTIAL.clone(), true);
        event
    }

    #[test]
    fn merge_passthorughs_non_partial_events() {
        let mut merge = Merge::from(MergeConfig::default());

        // A non-partial event.
        let sample_event = Event::from("hello world");

        // Once processed by the transform.
        let merged_event = merge.transform(sample_event.clone()).unwrap();

        // Should be returned as is.
        assert_eq!(merged_event, sample_event);
    }

    #[test]
    fn merge_merges_partial_events() {
        let mut merge = Merge::from(MergeConfig::default());

        let partial_event_1 = make_partial(Event::from("hel"));
        let partial_event_2 = make_partial(Event::from("lo "));
        let non_partial_event = Event::from("world");

        assert!(merge.transform(partial_event_1).is_none());
        assert!(merge.transform(partial_event_2).is_none());
        let merged_event = merge.transform(non_partial_event).unwrap();

        assert_eq!(
            merged_event
                .as_log()
                .get(&Atom::from("message"))
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"hello world"
        );

        // Merged event shouldn't contain partial event marker.
        assert!(!merged_event.as_log().contains(&event::PARTIAL));
    }

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
            let mut event = Event::new_empty_log();
            let log = event.as_mut_log();
            log.insert_implicit("merge", "hello "); // will be concatenated with the `merged` from `incoming`.
            log.insert_implicit("do_not_merge", "my_first_value"); // will remain as is, since it's not selected for merging.

            log.insert_implicit("merge_a", true); // will be overwritten with the `merge_a` from `incoming` (since it's a non-bytes kind).
            log.insert_implicit("merge_b", 123); // will be overwritten with the `merge_b` from `incoming` (since it's a non-bytes kind).

            log.insert_implicit("a", true); // will remain as is since it's not selected for merge.
            log.insert_implicit("b", 123); // will remain as is since it's not selected for merge.

            // `c` is not present in the `current`, and not selected for merge,
            // so it won't be included in the final event.

            event
        };

        let incoming = {
            let mut event = Event::new_empty_log();
            let log = event.as_mut_log();
            log.insert_implicit("merge", "world"); // will be concatenated to the `merge` from `current`.
            log.insert_implicit("do_not_merge", "my_second_value"); // will be ignored, since it's not selected for merge.

            log.insert_implicit("merge_b", 456); // will be merged in as `456`.
            log.insert_implicit("merge_c", false); // will be merged in as `false`.

            // `a` will remain as is, since it's not marked for merge and
            // niether it is specified in the `incoming` event.
            log.insert_implicit("b", 456); // `b` not marked for merge, will not change.
            log.insert_implicit("c", true); // `c` not marked for merge, will be ignored.

            event
        };

        let mut merged = current.clone();
        merge_event(&mut merged, incoming, &fields_to_merge);

        let expected = {
            let mut event = Event::new_empty_log();
            let log = event.as_mut_log();
            log.insert_implicit("merge", "hello world");
            log.insert_implicit("do_not_merge", "my_first_value");
            log.insert_implicit("a", true);
            log.insert_implicit("b", 123);
            log.insert_implicit("merge_a", true);
            log.insert_implicit("merge_b", 456);
            log.insert_implicit("merge_c", false);
            event
        };

        assert_eq!(merged, expected);
    }
}
