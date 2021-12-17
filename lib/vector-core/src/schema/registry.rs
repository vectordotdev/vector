use crate::schema;
use lookup::LookupBuf;
use std::collections::HashMap;

use super::field;

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
        self.input_definition.purpose()
    }

    pub fn input_kind(&self) -> &value::Kind {
        self.input_definition.kind()
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
