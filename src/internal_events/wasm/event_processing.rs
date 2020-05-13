use super::State;
use crate::{emit, internal_events::InternalEvent};
use metrics::counter;
use vector_wasm::Role;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct EventProcessing {
    role: Role,
    state: State,
}

impl EventProcessing {
    pub fn begin(role: Role) -> Self {
        let me = Self {
            state: State::Beginning,
            role,
        };
        emit!(me);
        me
    }
    pub fn complete(self) {
        emit!(Self {
            state: State::Completed,
            role: self.role,
        })
    }
}

impl InternalEvent for EventProcessing {
    fn emit_logs(&self) {
        trace!(
            message = "WASM Event Processing",
            state = self.state.as_const_str(),
            role = self.role.as_const_str(),
        );
    }

    fn emit_metrics(&self) {
        counter!("wasm_event_processing", 1,
            "component_kind" => self.role.as_const_str(),
            "component_type" => "wasm",
            "state" => self.state.as_const_str(),
        );
    }
}
