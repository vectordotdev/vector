use vector_common::TimeZone;

use crate::{state::Runtime, Target};

pub struct Context<'a> {
    target: &'a mut dyn Target,
    state: &'a mut Runtime,
    timezone: &'a TimeZone,
    /// If this value is `true`, the error can be discarded since it's not read by the parent
    /// expression.
    ///
    /// This allows for some optimizations to construct cheaper `Err` values, e.g. in the left hand
    /// side of the `??` operator.
    discard_error: bool,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(target: &'a mut dyn Target, state: &'a mut Runtime, timezone: &'a TimeZone) -> Self {
        Self {
            target,
            state,
            timezone,
            discard_error: false,
        }
    }

    /// Get a reference to the [`Target`].
    #[must_use]
    pub fn target(&self) -> &dyn Target {
        self.target
    }

    /// Get a mutable reference to the [`Target`].
    pub fn target_mut(&mut self) -> &mut dyn Target {
        self.target
    }

    /// Get a reference to the [`runtime state`](Runtime).
    #[must_use]
    pub fn state(&self) -> &Runtime {
        self.state
    }

    /// Get a mutable reference to the [`runtime state`](Runtime).
    pub fn state_mut(&mut self) -> &mut Runtime {
        self.state
    }

    /// Get a reference to the [`TimeZone`].
    #[must_use]
    pub fn timezone(&self) -> &TimeZone {
        self.timezone
    }

    /// Probe if errors should be discarded in this `Context`.
    #[must_use]
    pub fn discard_error(&self) -> bool {
        self.discard_error
    }

    /// Set if errors should be discarded in this `Context`.
    pub fn set_discard_error(&mut self, discard_error: bool) {
        self.discard_error = discard_error;
    }
}
