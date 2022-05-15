use std::{
    cell::{Ref, RefCell, RefMut},
    rc::Rc,
};

use vector_common::TimeZone;

use crate::{state::Runtime, Target};

pub struct Context<'a> {
    target: Rc<RefCell<dyn Target>>,
    state: RefCell<Runtime>,
    timezone: &'a TimeZone,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(
        target: Rc<RefCell<dyn Target>>,
        state: RefCell<Runtime>,
        timezone: &'a TimeZone,
    ) -> Self {
        Self {
            target,
            state,
            timezone,
        }
    }

    /// Get a reference to the [`Target`].
    pub fn target(&self) -> Ref<dyn Target> {
        self.target.borrow()
    }

    /// Get a mutable reference to the [`Target`].
    pub fn target_mut(&self) -> RefMut<dyn Target> {
        self.target.borrow_mut()
    }

    /// Get a reference to the [`runtime state`](Runtime).
    pub fn state(&self) -> Ref<Runtime> {
        self.state.borrow()
    }

    /// Get a mutable reference to the [`runtime state`](Runtime).
    pub fn state_mut(&self) -> RefMut<Runtime> {
        self.state.borrow_mut()
    }

    /// Get a reference to the [`TimeZone`]
    pub fn timezone(&self) -> &TimeZone {
        self.timezone
    }
}
