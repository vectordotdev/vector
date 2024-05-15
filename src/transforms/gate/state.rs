use crate::sinks::prelude::configurable_component;

/// Gate state
#[configurable_component]
#[derive(Clone, Debug, Copy)]
#[derive(PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum GateState {
    /// Gate is open
    Open,

    /// Gate is closed
    Closed,
}
