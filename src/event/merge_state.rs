use crate::event::{LogEvent, LookupBuf};

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
    pub fn merge_in_next_event(&mut self, incoming: LogEvent, fields: &[LookupBuf]) {
        self.intermediate_merged_event.merge(incoming, fields);
    }

    /// Merge the final (non-partial) event in and return the resulting (merged)
    /// event.
    pub fn merge_in_final_event(mut self, incoming: LogEvent, fields: &[LookupBuf]) -> LogEvent {
        self.merge_in_next_event(incoming, fields);
        self.intermediate_merged_event
    }
}

#[cfg(test)]
mod test {
    use super::LogEventMergeState;
    use crate::{
        event::{LogEvent, Lookup, LookupBuf},
        log_event,
    };

    fn log_event_with_message(message: &str) -> LogEvent {
        let event = log_event! {
            crate::config::log_schema().message_key().clone() => message.to_string(),
            crate::config::log_schema().timestamp_key().clone() => chrono::Utc::now(),
        };
        event.into_log()
    }

    #[test]
    fn log_event_merge_state_example() {
        let fields = vec![LookupBuf::from("message")];

        let mut state = LogEventMergeState::new(log_event_with_message("hel"));
        state.merge_in_next_event(log_event_with_message("lo "), &fields);
        let merged_event = state.merge_in_final_event(log_event_with_message("world"), &fields);

        assert_eq!(
            merged_event
                .get(Lookup::from("message"))
                .unwrap()
                .clone_into_bytes()
                .as_ref(),
            b"hello world"
        );
    }
}
