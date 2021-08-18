use crate::{enrichment_tables::EnrichmentTableSearch, state::Runtime, Target};
use shared::TimeZone;

pub struct Context<'a> {
    target: &'a mut dyn Target,
    state: &'a mut Runtime,
    timezone: &'a TimeZone,
    enrichment_tables: &'a Option<Box<dyn EnrichmentTableSearch>>,
}

impl<'a> Context<'a> {
    /// Create a new [`Context`].
    pub fn new(
        target: &'a mut dyn Target,
        state: &'a mut Runtime,
        timezone: &'a TimeZone,
        enrichment_tables: &'a Option<Box<dyn EnrichmentTableSearch>>,
    ) -> Self {
        Self {
            target,
            state,
            timezone,
            enrichment_tables,
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
        &mut self.state
    }

    pub fn get_enrichment_tables(&self) -> &Option<Box<dyn EnrichmentTableSearch>> {
        self.enrichment_tables
    }

    /// Get a reference to the [`TimeZone`]
    pub fn timezone(&self) -> &TimeZone {
        self.timezone
    }
}
