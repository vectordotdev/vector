use super::field;
use crate::schema;
use lookup::LookupBuf;
use once_cell::sync::OnceCell;
use std::{collections::HashMap, num::NonZeroU16};

static REGISTRY: OnceCell<Registry> = OnceCell::new();

/// Initialize the global schema registry.
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

    /// A list of schema requirements.
    requirements: HashMap<schema::Id, schema::Requirement>,

    /// Keep track of the next available schema identifier.
    ///
    /// The 0th identifier is reserved for the default "open" schema.
    next_id: NonZeroU16,
}

impl Registry {
    /// Register a new schema definition in the schema registry.
    ///
    /// The returned value is an identifier that can be used to later fetch the relevant schema.
    ///
    /// # Errors
    ///
    /// An error is returned if the schema holds the maximum amount of schemas.
    pub fn register_definition(
        &mut self,
        schema: schema::Definition,
    ) -> Result<schema::Id, String> {
        let id = schema::Id::from(self.next_id);
        self.definitions.insert(id, schema);

        self.next_id = self
            .next_id
            .get()
            .checked_add(1)
            .map(|i| NonZeroU16::new(i).expect("cannot be 0"))
            .ok_or_else(|| "schema registry full".to_owned())?;

        Ok(id)
    }

    /// Register a new schema requirement in the schema registry.
    ///
    /// The returned value is an identifier that can be used to later fetch the relevant schema.
    ///
    /// # Errors
    ///
    /// An error is returned if the schema holds the maximum amount of schemas.
    pub fn register_requirement(
        &mut self,
        schema: schema::Requirement,
    ) -> Result<schema::Id, String> {
        let id = schema::Id::from(self.next_id);
        self.requirements.insert(id, schema);

        self.next_id = self
            .next_id
            .get()
            .checked_add(1)
            .map(|i| NonZeroU16::new(i).expect("cannot be 0"))
            .ok_or_else(|| "schema registry full".to_owned())?;

        Ok(id)
    }

    /// Get an immutable reference to the schema definition matching the given Id.
    pub fn definition(&self, id: schema::Id) -> Option<&schema::Definition> {
        self.definitions.get(&id)
    }

    /// Get an immutable reference to the schema requirement matching the given Id.
    pub fn requirement(&self, id: schema::Id) -> Option<&schema::Requirement> {
        self.requirements.get(&id)
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self {
            definitions: HashMap::from([(schema::Id::empty(), schema::Definition::empty())]),
            requirements: HashMap::from([(schema::Id::empty(), schema::Requirement::empty())]),
            next_id: NonZeroU16::new(1).unwrap(),
        }
    }
}

/// A registry of schema information used by sinks to configure their encoders.
pub struct SinkRegistry {}

#[derive(Debug, Clone, Default)]
pub struct TransformRegistry {
    /// The list of sink schemas to which this transform has to adhere.
    requirements: Vec<schema::Requirement>,

    /// A merged schema definition of all components feeding into the transform.
    input_definition: schema::Definition,

    /// If `true`, the pipeline is considered to be in an incomplete state, which means calling
    /// some functions is disallowed, and others return "no-op" responses.
    ///
    /// This is in support of two-pass systems that need access to the current state of the schema
    /// registry before it has finished loading.
    loading: bool,
}

impl TransformRegistry {
    pub fn new(
        requirements: Vec<schema::Requirement>,
        input_definition: schema::Definition,
    ) -> Self {
        Self {
            requirements,
            input_definition,
            loading: true,
        }
    }

    pub fn finalize(mut self) -> Self {
        self.loading = false;
        self
    }

    pub fn is_loading(&self) -> bool {
        self.loading
    }

    pub fn is_valid_purpose(&self, purpose: &str) -> bool {
        self.requirements.iter().any(|s| {
            let purpose: field::Purpose = purpose.to_owned().into();

            s.purposes().contains(&&purpose)
        })
    }

    pub fn required_purposes(&self) -> Vec<&field::Purpose> {
        self.requirements
            .iter()
            .flat_map(schema::Requirement::purposes)
            .collect()
    }

    pub fn input_purposes(&self) -> &HashMap<field::Purpose, LookupBuf> {
        self.input_definition.purposes()
    }

    pub fn input_kind(&self) -> value::Kind {
        self.input_definition.to_kind()
    }

    pub fn input_definition(&self) -> schema::Definition {
        self.input_definition.clone()
    }

    pub fn register_purpose(&mut self, field: LookupBuf, purpose: &str) {
        self.input_definition
            .purpose_mut()
            .insert(purpose.into(), field);
    }
}
