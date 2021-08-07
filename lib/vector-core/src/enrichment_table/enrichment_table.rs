pub use vrl_core::EnrichmentTable;

/*
pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row<'a>(&'a self, criteria: BTreeMap<String, String>) -> Option<&'a Vec<String>>;
    fn add_index(&mut self, fields: Vec<&str>);
}

// Glue the Vector EnrichmentTable to the VRL table.
impl<T> vrl_core::EnrichmentTable for T
where
    T: EnrichmentTable,
{
    fn find_table_row<'a>(
        &'a self,
        criteria: std::collections::BTreeMap<String, String>,
    ) -> Option<&'a Vec<String>> {
        self.find_table_row(criteria)
    }

    fn add_index(&mut self, fields: Vec<&str>) {
        self.add_index(fields);
    }
}
*/
