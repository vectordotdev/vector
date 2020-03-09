use crate::event::merge::merge_log_event;
use crate::event::LogEvent;
use string_cache::DefaultAtom as Atom;

/// Encapsulates the inductive events merging algorithm.
///
/// In the future, this might be extended by various counters (the number of
/// events that contributed to the current merge event for instance, or the
/// event size) to support curcut breaker logic.
#[derive(Debug)]
pub struct LogEventMergeState {
    /// Intermediate event we merge into.
    intermediate_merged_event: LogEvent,
}

impl LogEventMergeState {
    /// Initialize the algorithm with a first (partial) event.
    pub fn new(first_partial_event: LogEvent) -> Self {
        Self {
            intermediate_merged_event: first_partial_event,
        }
    }

    /// Merge the incoming (partial) event in.
    pub fn merge_in_next_event(&mut self, incoming: LogEvent, merge_fields: &[Atom]) {
        merge_log_event(&mut self.intermediate_merged_event, incoming, merge_fields);
    }

    /// Merge the final (non-partial) event in and return the resulting (merged)
    /// event.
    pub fn merge_in_final_event(mut self, incoming: LogEvent, merge_fields: &[Atom]) -> LogEvent {
        self.merge_in_next_event(incoming, merge_fields);
        self.intermediate_merged_event
    }
}

#[cfg(test)]
mod test {
    use super::LogEventMergeState;
    use crate::event::{Event, LogEvent};
    use string_cache::DefaultAtom as Atom;

    fn log_event_with_message(message: &str) -> LogEvent {
        Event::from(message).into_log()
    }

    #[test]
    fn log_event_merge_state_example() {
        let merge_fields = &[Atom::from("message")];

        let mut state = LogEventMergeState::new(log_event_with_message("hel"));
        state.merge_in_next_event(log_event_with_message("lo "), merge_fields);
        let merged_event =
            state.merge_in_final_event(log_event_with_message("world"), merge_fields);

        assert_eq!(
            merged_event
                .get(&Atom::from("message"))
                .unwrap()
                .as_bytes()
                .as_ref(),
            b"hello world"
        );
    }
}
