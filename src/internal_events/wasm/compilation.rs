use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
#[cfg(feature = "wasm-timings")]
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
            role: self.role,
            error: self.error,
            epoch: self.epoch,
            elapsed: self.epoch.elapsed()
        })
    }
    pub fn error(self, error: String) {
        emit!(Self {
            state: State::Cached,
            role: self.role,
            error: Some(error),
            epoch: self.epoch,
            elapsed: self.epoch.elapsed()
        })
    }
    pub fn cached(self) {
        emit!(Self {
            state: State::Cached,
            role: self.role,
            error: self.error,
            epoch: self.epoch,
            elapsed: self.epoch.elapsed()
        })
    }
}

impl InternalEvent for WasmCompilationProgress {
    fn emit_logs(&self) {
        match self.state {
            State::Beginning | State::Cached | State::Completed => event!(
                tracing::Level::INFO,
                {
                    state = self.state.as_const_str(),
                    role = self.role.as_const_str(),
                    elapsed_micros = self.elapsed.as_micros() as u64,
                },
                "WASM Compilation via `lucet`.",
            ),
            State::Errored => event!(
                tracing::Level::ERROR,
                {
                    state = self.state.as_const_str(),
                    role = self.role.as_const_str(),
                    error = tracing::field::display(self.error.as_ref().unwrap_or(&String::from(""))),
                    elapsed_micros = self.elapsed.as_micros() as u64,
                    // We do not rate limit this since it should never spam, it's a oneshot at startup.
                },
                "WASM Compilation via `lucet`.",
            ),
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_compilation", 1,
            "component_kind" => self.role.as_const_str(),
            "component_type" => "wasm",
            "state" => self.state.as_const_str(),
        );
    }
}
