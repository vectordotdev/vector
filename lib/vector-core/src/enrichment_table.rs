use std::collections::BTreeMap;

pub trait EnrichmentTable: std::fmt::Debug {
    fn find_table_row<'a>(&'a self, criteria: BTreeMap<String, String>) -> Option<&'a Vec<String>>;
}
