use super::Role;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// A module registration.
#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
#[must_use]
#[repr(C)]
pub struct Registration {
    /// The role of the module.
    ///
    /// The host will also define this, and the registration will fail if they differ in types.
    /// This is a simple two-way handshake safety procedure to ensure modules get used in the right place.
    role: Role,
}

impl Registration {
    pub fn transform() -> Self {
        Self {
            role: Role::Transform,
        }
    }
    pub fn role(&self) -> Role {
        self.role
    }
    pub fn register(&self) -> Result<()> {
        super::hostcall::register(self)
    }
}
