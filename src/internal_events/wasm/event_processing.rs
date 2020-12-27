use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
use std::time::{Duration, Instant};
use vector_wasm::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct EventProcessingProgress {
    role: Role,
    state: State,
    error: Option<String>,
    epoch: Instant,
    elapsed: Duration,
}

impl EventProcessingProgress {
    pub fn begin(role: Role) -> Self {
        let me = Self {
            state: State::Beginning,
            role,
            error: Default::default(),
            epoch: Instant::now(),
            elapsed: Default::default(),
        };
        emit!(me.clone());
        me
    }

    pub fn error(self, error: String) {
        emit!(Self {
            state: State::Errored,
            error: Some(error),
            elapsed: self.epoch.elapsed(),
            ..self
        })
    }

    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            elapsed: self.epoch.elapsed(),
            ..self
        })
    }
}

impl InternalEvent for EventProcessingProgress {
    fn emit_logs(&self) {
        match self.state {
            State::Beginning | State::Cached | State::Completed => trace!(
                state = self.state.as_const_str(),
                role = self.role.as_const_str(),
                elapsed_micros = self.elapsed.as_micros() as u64,
                "Event processed.",
            ),
            State::Errored => error!(
                state = self.state.as_const_str(),
                role = self.role.as_const_str(),
                error = ?self.error.as_ref().unwrap_or(&String::from("")),
                elapsed_micros = self.elapsed.as_micros() as u64,
                internal_log_rate_secs = 30,
                "Event processing error.",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_event_processing_total", 1,
            "component_role" => self.role.as_const_str(),
            "state" => self.state.as_const_str(),
        );
        match self.state {
            State::Completed => counter!("wasm_processed_events_total", 1,
                "component_role" => self.role.as_const_str(),
            ),
            State::Errored => counter!("processing_errors_total", 1,
                "component_role" => self.role.as_const_str(),
            ),
            _ => (),
        }
    }
}
