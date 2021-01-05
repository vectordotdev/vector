use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
use std::time::{Duration, Instant};
use vector_wasm::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct WasmHostcallProgress {
    call: &'static str,
    role: Role,
    state: State,
    // This is expensive, it's only ok since it's a branch for errors.
    error: Option<String>,
    epoch: Instant,
    elapsed: Duration,
}

impl WasmHostcallProgress {
    pub fn begin(role: Role, call: &'static str) -> Self {
        let me = Self {
            state: State::Beginning,
            call,
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

impl InternalEvent for WasmHostcallProgress {
    fn emit_logs(&self) {
        match self.state {
            State::Beginning | State::Cached | State::Completed => trace!(
                state = self.state.as_const_str(),
                call = self.call,
                role = self.role.as_const_str(),
                elapsed_micros = self.elapsed.as_micros() as u64,
                "WASM Hostcall invocation.",
            ),
            State::Errored => error!(
                state = self.state.as_const_str(),
                call = self.call,
                role = self.role.as_const_str(),
                error = ?self.error.as_ref().unwrap_or(&String::from("")),
                elapsed_micros = self.elapsed.as_micros() as u64,
                internal_log_rate_secs = 30,
                "Hostcall errored.",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_hostcall_total", 1,
            "component_role" => self.role.as_const_str(),
            "state" => self.state.as_const_str(),
            "call" => self.call,
        );
    }
}
