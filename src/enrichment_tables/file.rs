use std::collections::BTreeMap;

use super::EnrichmentTable;
use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
struct FileConfig {
    filename: String,
    include_headers: bool,
    delimiter: char,
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn super::EnrichmentTable + Send + Sync>> {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(self.include_headers)
            .delimiter(self.delimiter as u8)
            .from_path(&self.filename)?;

        let data = reader
            .records()
            .map(|row| Ok(row?.iter().map(|col| col.to_string()).collect::<Vec<_>>()))
            .collect::<crate::Result<Vec<_>>>()?;

        let headers = if self.include_headers {
            reader
                .headers()?
                .iter()
                .map(|col| col.to_string())
                .collect::<Vec<_>>()
        } else {
            // If there are no headers in the datafile we make headers as the numerical index of
            // the column.
            match data.get(0) {
                Some(row) => (0..row.len()).map(|idx| idx.to_string()).collect(),
                None => Vec::new(),
            }
        };

        Ok(Box::new(File {
            data,
            headers,
            indexes: Vec::new(),
        }))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

struct File {
    data: Vec<Vec<String>>,
    headers: Vec<String>,
    indexes: Vec<Vec<String>>,
}

impl File {
    fn column_index(&self, col: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == col)
    }

    fn row_equals(&self, criteria: &BTreeMap<&str, String>, row: &[String]) -> bool {
        criteria
            .iter()
            .all(|(col, value)| match self.column_index(col) {
                None => false,
                Some(idx) => row[idx] == *value,
            })
    }

    fn add_columns(&self, row: &[String]) -> BTreeMap<String, String> {
        self.headers
            .iter()
            .zip(row)
            .map(|(header, col)| (header.clone(), col.clone()))
            .collect()
    }
}

impl EnrichmentTable for File {
    fn find_table_row(&self, criteria: BTreeMap<&str, String>) -> Option<BTreeMap<String, String>> {
        // Sequential scan
        let results = self
            .data
            .iter()
            .find(|row| self.row_equals(&criteria, *row))
            .map(|row| self.add_columns(row));

        results
    }

    fn add_index(&mut self, fields: Vec<&str>) {
        self.indexes
            .push(fields.iter().map(ToString::to_string).collect());
    }
}

impl std::fmt::Debug for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "File {} row(s) {} index(es)",
            self.data.len(),
            self.indexes.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::btreemap;

    #[test]
    fn finds_row() {
        let file = File {
            data: vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            headers: vec!["field1".to_string(), "field2".to_string()],
            indexes: vec![],
        };

        let condition = btreemap! {
            "field1" => "zirp"
        };

        assert_eq!(
            Some(btreemap! {
                "field1" => "zirp",
                "field2" => "zurp",
            }),
            file.find_table_row(condition)
        );
    }

    #[test]
    fn doesnt_find_row() {
        let file = File {
            data: vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            headers: vec!["field1".to_string(), "field2".to_string()],
            indexes: vec![],
        };

        let condition = btreemap! {
            "field1" => "zorp"
        };

        assert_eq!(None, file.find_table_row(condition));
    }
}
