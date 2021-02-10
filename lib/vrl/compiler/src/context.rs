use crate::{state::Runtime, Target};

pub struct Context<'a> {
    target: &'a mut dyn Target,
    state: &'a mut Runtime,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(target: &'a mut dyn Target, state: &'a mut Runtime) -> Self {
        Self { target, state }
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
        &self.state
    }

    /// Get a mutable reference to the [`runtime state`](Runtime).
    pub fn state_mut(&mut self) -> &mut Runtime {
        &mut self.state
    }
}
