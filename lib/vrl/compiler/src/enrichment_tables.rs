use dyn_clone::DynClone;

pub enum Condition {
    Equals { field: String, value: String },
}

pub trait EnrichmentTableSetup {
    fn table_ids(&self) -> Vec<String>;
    fn add_index(&mut self, table: &str, fields: &[&str]) -> Result<(), String>;
}

pub trait EnrichmentTableSearch: DynClone {
    fn find_table_row(
        &self,
        table: &str,
        criteria: Vec<Condition>,
    ) -> Result<Option<Vec<String>>, String>;
}

dyn_clone::clone_trait_object!(EnrichmentTableSearch);

/// Create a empty enrichment for situations when we don't have any tables loaded.
#[derive(Clone, Debug)]
pub struct EmptyEnrichmentTables;

impl EnrichmentTableSetup for EmptyEnrichmentTables {
    fn table_ids(&self) -> Vec<String> {
        Vec::new()
    }

    fn add_index(&mut self, _table: &str, _fields: &[&str]) -> Result<(), String> {
        Ok(())
    }
}

impl EnrichmentTableSearch for EmptyEnrichmentTables {
    fn find_table_row(
        &self,
        _table: &str,
        _criteria: Vec<Condition>,
    ) -> Result<Option<Vec<String>>, String> {
        Ok(None)
    }
}
