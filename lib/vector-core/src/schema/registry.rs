use crate::schema;
use lookup::LookupBuf;
use std::collections::HashMap;

use super::field;

#[derive(Debug, Clone, Default)]
pub struct TransformRegistry {
    /// The list of sink schemas to which this transform has to adhere.
    sink_schemas: Vec<schema::Input>,

    /// A merged schema definition of all components feeding into the transform.
    received_schema: schema::Output,

    /// If `true`, the pipeline is considered to be in an incomplete state, which means calling
    /// some functions is disallowed, and others return "no-op" responses.
    ///
    /// This is in support of two-pass systems that need access to the current state of the schema
    /// registry before it has finished loading.
    loading: bool,
}

impl TransformRegistry {
    pub fn new(sink_schemas: Vec<schema::Input>, received_schema: schema::Output) -> Self {
        Self {
            sink_schemas,
            received_schema,
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
        self.sink_schemas.iter().any(|s| {
            let purpose: field::Purpose = purpose.to_owned().into();

            s.purposes().contains(&&purpose)
        })
    }

    pub fn sink_purposes(&self) -> Vec<&field::Purpose> {
        self.sink_schemas
            .iter()
            .flat_map(|s| s.purposes())
            .collect()
    }

    pub fn input_purpose(&self) -> &HashMap<field::Purpose, LookupBuf> {
        self.received_schema.purpose()
    }

    pub fn input_kind(&self) -> &value::Kind {
        self.received_schema.kind()
    }

    pub fn register_purpose(&mut self, field: LookupBuf, purpose: &str) {
        self.received_schema
            .purpose_mut()
            .insert(purpose.into(), field);
    }
}
