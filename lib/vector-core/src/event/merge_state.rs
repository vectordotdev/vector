use super::LogEvent;

/// Encapsulates the inductive events merging algorithm.
///
/// In the future, this might be extended by various counters (the number of
/// events that contributed to the current merge event for instance, or the
/// event size) to support circuit breaker logic.
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
    pub fn merge_in_next_event(&mut self, incoming: LogEvent, fields: &[impl AsRef<str>]) {
        self.intermediate_merged_event.merge(incoming, fields);
    }

    /// Merge the final (non-partial) event in and return the resulting (merged)
    /// event.
    pub fn merge_in_final_event(
        mut self,
        incoming: LogEvent,
        fields: &[impl AsRef<str>],
    ) -> LogEvent {
        self.merge_in_next_event(incoming, fields);
        self.intermediate_merged_event
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn log_event_with_message(message: &str) -> LogEvent {
        LogEvent::from(message)
    }

    #[test]
    fn log_event_merge_state_example() {
        let fields = vec!["message".to_string()];

        let mut state = LogEventMergeState::new(log_event_with_message("hel"));
        state.merge_in_next_event(log_event_with_message("lo "), &fields);
        let merged_event = state.merge_in_final_event(log_event_with_message("world"), &fields);

        assert_eq!(
            merged_event
                .get("message")
                .unwrap()
                .coerce_to_bytes()
                .as_ref(),
            b"hello world"
        );
    }
}
