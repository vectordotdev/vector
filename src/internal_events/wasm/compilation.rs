use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
use std::time::{Duration, Instant};
use vector_wasm::Role;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct WasmCompilationProgress {
    role: Role,
    state: State,
    error: Option<String>,
    epoch: Instant,
    elapsed: Duration,
}

impl WasmCompilationProgress {
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

    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            elapsed: self.epoch.elapsed(),
            ..self
        })
    }

    pub fn error(self, error: String) {
        emit!(Self {
            state: State::Cached,
            error: Some(error),
            elapsed: self.epoch.elapsed(),
            ..self
        })
    }

    pub fn cached(self) {
        emit!(Self {
            state: State::Cached,
            elapsed: self.epoch.elapsed(),
            ..self
        })
    }
}

impl InternalEvent for WasmCompilationProgress {
    fn emit_logs(&self) {
        match self.state {
            State::Beginning | State::Cached | State::Completed => info!(
                state = self.state.as_const_str(),
                role = self.role.as_const_str(),
                elapsed_micros = self.elapsed.as_micros() as u64,
                "WASM Compilation via `lucet`.",
            ),
            State::Errored => error!(
                state = self.state.as_const_str(),
                role = self.role.as_const_str(),
                error = %self.error.as_ref().unwrap_or(&String::from("")),
                elapsed_micros = self.elapsed.as_micros() as u64,
                // We do not rate limit this since it should never spam, it's a oneshot at startup.
                "WASM Compilation via `lucet`.",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_compilation", 1,
            "component_role" => self.role.as_const_str(),
            "state" => self.state.as_const_str(),
        );
    }
}
