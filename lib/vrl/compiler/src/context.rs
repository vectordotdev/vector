use vector_common::TimeZone;

use crate::{state::Runtime, Target};

pub struct Context<'a> {
    target: &'a mut dyn Target,
    state: &'a mut Runtime,
    timezone: &'a TimeZone,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(target: &'a mut dyn Target, state: &'a mut Runtime, timezone: &'a TimeZone) -> Self {
        Self {
            target,
            state,
            timezone,
        }
    }

    /// Get a reference to the [`Target`].
    pub fn target(&self) -> &dyn Target {
        self.target
    }

    /// Get a mutable reference to the [`Target`].
    pub fn target_mut(&mut self) -> &mut dyn Target {
        self.target
    }

    /// Get a reference to the [`runtime state`](Runtime).
    pub fn state(&self) -> &Runtime {
        self.state
    }

    /// Get a mutable reference to the [`runtime state`](Runtime).
    pub fn state_mut(&mut self) -> &mut Runtime {
        self.state
    }

    /// Get a reference to the [`TimeZone`]
    pub fn timezone(&self) -> &TimeZone {
        self.timezone
    }
}
