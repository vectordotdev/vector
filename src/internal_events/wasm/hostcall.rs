use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
#[cfg(feature = "wasm-timings")]
use std::time::{Duration, Instant};
use vector_wasm::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Hostcall {
    call: &'static str,
    role: Role,
    state: State,
    #[cfg(feature = "wasm-timings")]
    epoch: Instant,
    #[cfg(feature = "wasm-timings")]
    elapsed: Duration,
}

impl Hostcall {
    pub fn begin(role: Role, call: &'static str) -> Self {
        let me = Self {
            state: State::Beginning,
            call,
            role,
            #[cfg(feature = "wasm-timings")]
            epoch: Instant::now(),
            #[cfg(feature = "wasm-timings")]
            elapsed: Default::default(),
        };
        emit!(me);
        me
    }
    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            call: self.call,
            role: self.role,
            #[cfg(feature = "wasm-timings")]
            epoch: self.epoch,
            #[cfg(feature = "wasm-timings")]
            elapsed: self.epoch.elapsed()
        })
    }
}

impl InternalEvent for Hostcall {
    fn emit_logs(&self) {
        #[cfg(not(feature = "wasm-timings"))]
        trace!(
            state = self.state.as_const_str(),
            call = self.call,
            role = self.role.as_const_str(),
        );
        #[cfg(feature = "wasm-timings")]
        {
            if self.elapsed.as_nanos() == 0 {
                trace!(
                    state = self.state.as_const_str(),
                    call = self.call,
                    role = self.role.as_const_str(),
                );
            } else {
                trace!(
                    state = self.state.as_const_str(),
                    call = self.call,
                    role = self.role.as_const_str(),
                    elapsed_micros = self.elapsed.as_micros() as u64,
                );
            }
        }
    }

    fn emit_metrics(&self) {
        counter!("wasm_hostcall", 1,
            "component_kind" => self.role.as_const_str(),
            "component_type" => "wasm",
            "state" => self.state.as_const_str(),
            "call" => self.call,
        );
    }
}
