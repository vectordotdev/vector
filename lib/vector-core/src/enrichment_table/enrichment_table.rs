use std::collections::BTreeMap;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row<'a>(
        &'a self,
        criteria: BTreeMap<String, String>,
    ) -> Option<&'a BTreeMap<String, vrl_core::Value>>;
    fn add_index(&mut self, fields: Vec<&str>);
}
