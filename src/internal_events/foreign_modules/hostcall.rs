use crate::{foreign_modules::Role, emit, internal_events::InternalEvent};
use metrics::counter;
use super::State;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct Hostcall {
    call: &'static str,
    role: Role,
    state: State,
}

impl Hostcall {
    pub fn begin(role: Role, call: &'static str) -> Self {
        let me = Self {
            state: State::Beginning,
            call,
            role,
        };
        emit!(me);
        me
    }
    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            call: self.call,
            role: self.role,
        })
    }
}

impl InternalEvent for Hostcall {
    fn emit_logs(&self) {
        debug!(
            message = "WASM hostcall",
            state = self.state.as_const_str(),
            call = self.call,
            role = self.role.as_const_str(),
        );
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
