use super::Transform;
use crate::{
    event::discriminant::Discriminant,
    event::merge_state::LogEventMergeState,
    event::{self, Event, LogEvent},
    topology::config::{DataType, TransformConfig, TransformContext, TransformDescription},
};
use serde::{Deserialize, Serialize};
use std::collections::{hash_map, HashMap};
use string_cache::DefaultAtom as Atom;

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(deny_unknown_fields, default)]
pub struct MergeConfig {
    /// The field that indicates that the event is partial. A consequent stream
    /// of partial events along with the first non-partial event will be merged
    /// together.
    pub partial_event_marker_field: Atom,
    /// Fields to merge. The values of these fields will be merged into the
    /// first partial event. Fields not specified here will be ignored.
    /// Merging process takes the first partial event and the base, then it
    /// merges in the fields from each successive partial event, until a
    /// non-partial event arrives. Finally, the non-partial event fields are
    /// merged in, producing the resulting merged event.
    pub merge_fields: Vec<Atom>,
    /// An ordered list of fields to distinguish streams by. Each stream has a
    /// separate partial event merging state. Should be used to prevent events
    /// from unrelated sources from mixing together, as this affects partial
    /// event processing.
    pub stream_discriminant_fields: Vec<Atom>,
}

inventory::submit! {
    TransformDescription::new::<MergeConfig>("merge")
}

impl Default for MergeConfig {
    fn default() -> Self {
        Self {
            partial_event_marker_field: event::PARTIAL.clone(),
            merge_fields: vec![event::log_schema().message_key().clone()],
            stream_discriminant_fields: vec![],
        }
    }
}

#[typetag::serde(name = "merge")]
impl TransformConfig for MergeConfig {
    fn build(&self, _cx: TransformContext) -> crate::Result<Box<dyn Transform>> {
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
pub struct Merge<N>
where
    N: NormalizeLogEvent,
{
    normalizer: N,
    merge_fields: Vec<Atom>,
    stream_discriminant_fields: Vec<Atom>,
    log_event_merge_states: HashMap<Discriminant, LogEventMergeState>,
}

impl From<MergeConfig> for Merge<PartialEventMarkerFieldNormalizer> {
    fn from(config: MergeConfig) -> Self {
        let MergeConfig {
            partial_event_marker_field,
            merge_fields,
            stream_discriminant_fields,
        } = config;

        Self::new(
            PartialEventMarkerFieldNormalizer {
                partial_event_marker_field,
            },
            merge_fields,
            stream_discriminant_fields,
        )
    }
}

impl<N> Merge<N>
where
    N: NormalizeLogEvent,
{
    /// Create a new [`Merge`] transform with the specified parameters.
    pub fn new(
        normalizer: N,
        merge_fields: Vec<Atom>,
        stream_discriminant_fields: Vec<Atom>,
    ) -> Self {
        Self {
            normalizer,
            merge_fields,
            stream_discriminant_fields,
            log_event_merge_states: HashMap::new(),
        }
    }
}

impl<N> Transform for Merge<N>
where
    N: NormalizeLogEvent + Send,
{
    fn transform(&mut self, event: Event) -> Option<Event> {
        let event = event.into_log();

        // Prepare an event's discriminant.
        let discriminant = Discriminant::from_log_event(&event, &self.stream_discriminant_fields);

        // TODO: `lua` transform doesn't support assigning non-string values.
        // Normally we'd check for the field value to be `true`, and only then
        // consider event partial, but, to simplify the integration, for now we
        // only check for the field presence. We can switch this to check the
        // value to be `true` when the `lua` supports setting boolean fields
        // easily, as we expect users to rely on `lua` transform to implement
        // custom partial markers.

        // Normalize the event, and perform partiality detection. Normalization
        // should clean up the event from the the partiality markers, if
        // applicable (and if it is a sane behaviuous for a particular use
        // case).
        // If the resulting normalized event is partial - stash it.
        let event = match self.normalizer.normalize(event) {
            MaybePartialLogEvent::Partial(event) => {
                // We got a perial event. Initialize a partial event merging state
                // if there's none available yet, or extend the existing one by
                // merging the incoming partial event in.
                match self.log_event_merge_states.entry(discriminant) {
                    hash_map::Entry::Vacant(entry) => {
                        entry.insert(LogEventMergeState::new(event));
                    }
                    hash_map::Entry::Occupied(mut entry) => {
                        entry
                            .get_mut()
                            .merge_in_next_event(event, &self.merge_fields);
                    }
                }

                // Do not emit the event yet.
                return None;
            }
            MaybePartialLogEvent::NonPartial(event) => event,
        };

        // We got non-partial event. Attempt to get a partial event merge
        // state. If it's empty then we don't have a backlog of partail events
        // so we just return the event as is. Otherwise we proceed to merge in
        // the final non-partial event to the partial event merge state - and
        // then return the merged event.
        let log_event_merge_state = match self.log_event_merge_states.remove(&discriminant) {
            Some(log_event_merge_state) => log_event_merge_state,
            None => return Some(Event::Log(event)),
        };

        // Merge in the final non-partial event and consume the merge state in
        // exchange for the merged event.
        let merged_event = log_event_merge_state.merge_in_final_event(event, &self.merge_fields);

        // Return the merged event.
        Some(Event::Log(merged_event))
    }
}

/// Represents either a partial or non-partial event.
/// In both cases, the actual underlying event is ready for further processing,
/// in a sense that, if the event is partial and the partial event marker has
/// to be removed according to the [`NormalizeLogEvent`] implementation
/// semantics, the event contained at the [`MaybePartialLogEvent`] already has
/// the partial event marker cleaned up.
#[derive(Debug)]
pub enum MaybePartialLogEvent {
    Partial(LogEvent),
    NonPartial(LogEvent),
}

/// Performs normalization of the event for merging purposes. It's also
/// responsible for determining whether the event is partial or non-partial.
///
/// If the event has a partial marker of any kind - that event is considered
/// partial, otherwise - non-partial.
/// The job of NormalizeLogEvent implementation if to determine whether the
/// event is partial, and if it is - clear the partial event marker from it and
/// return the resulting cleared event as [`MaybePartialLogEvent::Partial`].
/// Events that are detected as non-partial have to be returned as
/// [`MaybePartialLogEvent::MonPartial`]; negative partial event marker, if any,
/// can optionally be removed from them.
///
/// It's the implementer's job to determine a sane normalization strategy, since
/// correct semantics varies on a case by case basis.
///
/// If you have troubles with determining the correct semantics, consider
/// checking existing implementations to see some examples.
pub trait NormalizeLogEvent {
    fn normalize(&self, event: LogEvent) -> MaybePartialLogEvent;
}

mod normalizer;
pub use normalizer::*;

#[cfg(test)]
mod test {
    use super::{Merge, MergeConfig};
    use crate::event::{self, Event};
    use crate::transforms::Transform;
    use string_cache::DefaultAtom as Atom;

    fn make_partial(mut event: Event) -> Event {
        event.as_mut_log().insert(event::PARTIAL.clone(), true);
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

    #[test]
    fn merge_merges_partial_events_from_separate_streams() {
        let stream_discriminant_field = Atom::from("stream_name");

        let mut merge = Merge::from(MergeConfig {
            stream_discriminant_fields: vec![stream_discriminant_field.clone()],
            ..MergeConfig::default()
        });

        let make_event = |message, stream| {
            let mut event = Event::from(message);
            event
                .as_mut_log()
                .insert(stream_discriminant_field.clone(), stream);
            event
        };

        let s1_partial_event_1 = make_partial(make_event("hel", "s1"));
        let s1_partial_event_2 = make_partial(make_event("lo ", "s1"));
        let s1_non_partial_event = make_event("world", "s1");

        let s2_partial_event_1 = make_partial(make_event("lo", "s2"));
        let s2_partial_event_2 = make_partial(make_event("rem ip", "s2"));
        let s2_non_partial_event = make_event("sum", "s2");

        // Simulate events arriving in non-trivial order.
        assert!(merge.transform(s1_partial_event_1).is_none());
        assert!(merge.transform(s2_partial_event_1).is_none());
        assert!(merge.transform(s1_partial_event_2).is_none());
        let s1_merged_event = merge.transform(s1_non_partial_event).unwrap();
        assert!(merge.transform(s2_partial_event_2).is_none());
        let s2_merged_event = merge.transform(s2_non_partial_event).unwrap();

        assert_eq!(
            s1_merged_event
                .as_log()
                .get(&Atom::from("message"))
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"hello world"
        );

        assert_eq!(
            s2_merged_event
                .as_log()
                .get(&Atom::from("message"))
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"lorem ipsum"
        );

        // Merged events shouldn't contain partial event marker.
        assert!(!s1_merged_event.as_log().contains(&event::PARTIAL));
        assert!(!s2_merged_event.as_log().contains(&event::PARTIAL));
    }
}
