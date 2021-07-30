use super::EnrichmentTable;
use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};
use serde::{Deserialize, Serialize};
use tracing::trace;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
struct FileConfig;

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn super::EnrichmentTable + Send + Sync>> {
        trace!("Building file enrichment table");
        Ok(Box::new(File {
            data: vec![vec!["field1".to_string(), "field2".to_string()]],
        }))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

struct File {
    data: Vec<Vec<String>>,
}

impl EnrichmentTable for File {
    fn find_table_row<'a>(
        &'a self,
        _criteria: std::collections::BTreeMap<String, String>,
    ) -> Option<&'a Vec<String>> {
        trace!("Searching enrichment table.");
        Some(&self.data[0])
    }
}
