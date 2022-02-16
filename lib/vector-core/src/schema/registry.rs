use crate::schema;
use once_cell::sync::OnceCell;
use std::{collections::HashMap, fmt, num::NonZeroU16};
use value::Kind;

static REGISTRY: OnceCell<Registry> = OnceCell::new();

/// Initialize the global schema registry.
///
/// TODO(Jean):
///
///   It'd be nice if we could thread the registry through the topology to the relevant
///   sources/sinks, without having to access a global static.
///
///   However, given the churn thus causes, and the fact that we already use the same global static
///   pattern for the (unrelated) `LOG_SCHEMA` (see `vector-core`), this is the easiest solution to
///   the problem.
///
///   We'll still migrate to a non-global solution eventually, just not at the moment.
///
/// # Panics
///
/// If trying to initialize the registry more than once.
pub fn init(registry: Registry) {
    REGISTRY
        .set(registry)
        .expect("must be registered exactly once");
}

/// Get an immutable reference to the global schema registry.
///
/// # Panics
///
/// If the registry is unitialized (see `init_registry`).
pub fn global() -> &'static Registry {
    REGISTRY.get().expect("must call `init_registry` first")
}

/// A global schema registry that tracks known event schemas and their information.
///
/// Each event gets assigned a [`schema::Id`], which corresponds to a schema existing in this
/// registry.
#[derive(Debug)]
pub struct Registry {
    /// A list of schema definitions.
    definitions: HashMap<schema::Id, schema::Definition>,

    /// Keep track of the next available schema identifier.
    ///
    /// The 0th identifier is reserved for the default "empty" schema.
    next_id: NonZeroU16,
}

impl Registry {
    /// Register a new schema definition in the schema registry.
    ///
    /// The returned value is an identifier that can be used to later fetch the relevant schema.
    ///
    /// # Errors
    ///
    /// An error is returned if the registry holds the maximum amount of schemas.
    pub fn register_definition(
        &mut self,
        definition: schema::Definition,
    ) -> Result<schema::Id, Error> {
        let id = schema::Id::from(self.next_id);
        self.definitions.insert(id, definition);

        self.next_id = self
            .next_id
            .get()
            .checked_add(1)
            .map(|i| NonZeroU16::new(i).expect("cannot be 0"))
            .ok_or(Error::RegistryFull)?;

        Ok(id)
    }

    /// Get an immutable reference to the schema definition matching the given Id.
    pub fn definition(&self, id: schema::Id) -> Option<&schema::Definition> {
        self.definitions.get(&id)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            definitions: HashMap::from([(
                schema::Id::empty(),
                schema::Definition::empty().unknown_fields(Kind::any()),
            )]),
            next_id: NonZeroU16::new(1).unwrap(),
        }
    }
}

/// Error states for the schema registry.
#[derive(Debug)]
pub enum Error {
    /// The registry is full, no new schemas can be registered.
    RegistryFull,
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::RegistryFull => f.write_str("schema registry full"),
        }
    }
}
