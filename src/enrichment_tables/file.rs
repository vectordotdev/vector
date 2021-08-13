use std::collections::{BTreeMap, HashMap};

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

        Ok(Box::new(File::new(IndexingStrategy::Hash, data, headers)))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

pub enum IndexingStrategy {
    None,
    Hash,
}

pub struct File {
    strategy: IndexingStrategy,
    data: Vec<Vec<String>>,
    headers: Vec<String>,
    indexes: Vec<(Vec<String>, HashMap<u64, Vec<usize>>)>,
}

impl File {
    pub fn new(strategy: IndexingStrategy, data: Vec<Vec<String>>, headers: Vec<String>) -> Self {
        Self {
            strategy,
            data,
            headers,
            indexes: Vec::new(),
        }
    }

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

    /// Creates an index with the given fields.
    /// Uses seahash to create a hash of the data that is stored in a hashmap.
    fn index_data(&self, index: Vec<&str>) -> HashMap<u64, Vec<usize>> {
        let fieldidx = self
            .headers
            .iter()
            .enumerate()
            .filter(|(_, col)| {
                let onk: &str = col.as_ref();
                index.contains(&onk)
            })
            .map(|(idx, _)| idx)
            .collect::<Vec<_>>();

        let mut index = HashMap::new();

        for (idx, row) in self.data.iter().enumerate() {
            let mut hash = Vec::new();
            for idx in &fieldidx {
                hash.extend(row[*idx].bytes());
                hash.push(0_u8);
            }

            let key = seahash::hash(&hash);

            let entry = index.entry(key).or_insert(Vec::new());
            entry.push(idx);
        }

        index
    }
}

impl EnrichmentTable for File {
    fn find_table_row(&self, criteria: BTreeMap<&str, String>) -> Option<BTreeMap<String, String>> {
        match self.strategy {
            IndexingStrategy::None => {
                // Sequential scan
                let mut found = self
                    .data
                    .iter()
                    .filter(|row| self.row_equals(&criteria, *row))
                    .map(|row| self.add_columns(row));

                let result = found.next();

                if found.next().is_some() {
                    // More than one row has been found.
                    None
                } else {
                    result
                }
            }
            IndexingStrategy::Hash => {
                // Hash lookup
                let mut fields = criteria
                    .iter()
                    .map(|(field, _)| *field)
                    .collect::<Vec<&str>>();

                fields.sort();

                let hash = criteria.iter().fold(Vec::new(), |mut hash, (_, value)| {
                    hash.extend(value.bytes());
                    hash.push(0_u8);
                    hash
                });

                let key = seahash::hash(&hash);

                self.indexes
                    .iter()
                    .find(|(ifields, _)| {
                        // Find the correct index to use based on the fields we are searching.
                        ifields
                            .iter()
                            .zip(fields.iter())
                            .all(|(left, right)| left == right)
                    })
                    .and_then(|(_, index)| {
                        // Lookup the index
                        index.get(&key)
                    })
                    .and_then(|rows| {
                        if rows.len() == 1 {
                            Some(self.add_columns(&self.data[rows[0]]))
                        } else {
                            None
                        }
                    })
            }
        }
    }

    fn add_index(&mut self, fields: Vec<&str>) {
        let mut indexfields: Vec<String> = fields.iter().map(|field| field.to_string()).collect();
        indexfields.sort();

        self.indexes.push((indexfields, self.index_data(fields)));
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
        let file = File::new(
            IndexingStrategy::None,
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

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
    fn finds_row_with_index() {
        let mut file = File::new(
            IndexingStrategy::Hash,
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        file.add_index(vec!["field1"]);

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
        let file = File::new(
            IndexingStrategy::None,
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let condition = btreemap! {
            "field1" => "zorp"
        };

        assert_eq!(None, file.find_table_row(condition));
    }

    #[test]
    fn doesnt_find_row_with_index() {
        let mut file = File::new(
            IndexingStrategy::Hash,
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        file.add_index(vec!["field1"]);

        let condition = btreemap! {
            "field1" => "zorp"
        };

        assert_eq!(None, file.find_table_row(condition));
    }
}
