use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
#[cfg(feature = "wasm-timings")]
use std::time::{Duration, Instant};
use vector_wasm::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct EventProcessingProgress {
    role: Role,
    state: State,
    error: Option<String>,
    #[cfg(feature = "wasm-timings")]
    epoch: Instant,
    #[cfg(feature = "wasm-timings")]
    elapsed: Duration,
}

impl EventProcessingProgress {
    pub fn begin(role: Role) -> Self {
        let me = Self {
            state: State::Beginning,
            role,
            error: Default::default(),
            #[cfg(feature = "wasm-timings")]
            epoch: Instant::now(),
            #[cfg(feature = "wasm-timings")]
            elapsed: Default::default(),
        };
        emit!(me.clone());
        me
    }
    pub fn error(self, error: String) {
        emit!(Self {
            state: State::Errored,
            role: self.role,
            error: Some(error),
            #[cfg(feature = "wasm-timings")]
            epoch: self.epoch,
            #[cfg(feature = "wasm-timings")]
            elapsed: self.epoch.elapsed()
        })
    }
    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            role: self.role,
            error: self.error,
            #[cfg(feature = "wasm-timings")]
            epoch: self.epoch,
            #[cfg(feature = "wasm-timings")]
            elapsed: self.epoch.elapsed()
        })
    }
}

impl InternalEvent for EventProcessingProgress {
    fn emit_logs(&self) {
        match self.state {
            State::Beginning | State::Cached | State::Completed => event!(
                tracing::Level::TRACE,
                {
                    state = self.state.as_const_str(),
                    role = self.role.as_const_str(),
                    elapsed_micros = self.elapsed.as_micros() as u64,
                },
                "Event processed.",
            ),
            State::Errored => event!(
                tracing::Level::ERROR,
                {
                    state = self.state.as_const_str(),
                    role = self.role.as_const_str(),
                    error = tracing::field::display(self.error.as_ref().unwrap_or(&String::from(""))),
                    elapsed_micros = self.elapsed.as_micros() as u64,
                    rate_limit_secs = 30,
                },
                "Event processing error.",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_event_processing", 1,
            "component_kind" => self.role.as_const_str(),
            "component_type" => "wasm",
            "state" => self.state.as_const_str(),
        );
        match self.state {
            State::Completed => counter!("events_processed", 1,
                "component_kind" => self.role.as_const_str(),
                "component_type" => "wasm",
            ),
            State::Errored => counter!("processing_errors", 1,
                "component_kind" => self.role.as_const_str(),
                "component_type" => "wasm",
            ),
            _ => (),
        }
    }
}
