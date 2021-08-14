use super::{EnrichmentTable, IndexHandle};
use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
struct FileConfig {
    filename: String,
    encoding: String,
    #[serde(default = "crate::serde::default_true")]
    include_headers: bool,
    #[serde(default = "default_delimiter")]
    delimiter: char,
}

fn default_delimiter() -> char {
    ','
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn super::EnrichmentTable + Send + Sync>> {
        if self.encoding != "csv" {
            return Err("Only csv encoding is currently supported.".into());
        }

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

        Ok(Box::new(File::new(data, headers)))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

pub struct File {
    data: Vec<Vec<String>>,
    headers: Vec<String>,
    // Indexes are tuple of the position within the header of the fields to a hashmap of the hash
    // of those fields to the index of the row found in the data.
    indexes: Vec<(Vec<usize>, BTreeMap<u64, Vec<usize>>)>,
}

impl File {
    pub fn new(data: Vec<Vec<String>>, headers: Vec<String>) -> Self {
        Self {
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
    fn index_data(&self, index: Vec<&str>) -> (Vec<usize>, BTreeMap<u64, Vec<usize>>) {
        let fieldidx = self
            .headers
            .iter()
            .enumerate()
            .filter_map(|(idx, col)| {
                if index.contains(&col.as_ref()) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut index = BTreeMap::new();

        for (idx, row) in self.data.iter().enumerate() {
            let mut hash = seahash::SeaHasher::default();
            for idx in &fieldidx {
                hash.write(row[*idx].as_bytes());
                hash.write_u8(0);
            }

            let key = hash.finish();

            let entry = index.entry(key).or_insert(Vec::new());
            entry.push(idx);
        }

        (fieldidx, index)
    }
}

impl EnrichmentTable for File {
    fn find_table_row(
        &self,
        criteria: BTreeMap<&str, String>,
        index: Option<IndexHandle>,
    ) -> Option<BTreeMap<String, String>> {
        match index {
            None => {
                // Sequential scan
                let mut found = self.data.iter().filter_map(|row| {
                    if self.row_equals(&criteria, &*row) {
                        Some(self.add_columns(row))
                    } else {
                        None
                    }
                });

                let result = found.next();

                if found.next().is_some() {
                    // More than one row has been found.
                    None
                } else {
                    result
                }
            }
            Some(IndexHandle(handle)) => {
                // Hash lookup
                let mut hash = seahash::SeaHasher::default();

                for field in self.headers.iter() {
                    match criteria.get(field as &str) {
                        Some(value) => {
                            hash.write(value.as_bytes());
                            hash.write_u8(0);
                        }
                        None => (),
                    }
                }

                let key = hash.finish();

                self.indexes[handle].1.get(&key).and_then(|rows| {
                    if rows.len() == 1 {
                        Some(self.add_columns(&self.data[rows[0]]))
                    } else {
                        None
                    }
                })
            }
        }
    }

    fn add_index(&mut self, fields: Vec<&str>) -> Result<IndexHandle, String> {
        self.indexes.push(self.index_data(fields));
        Ok(IndexHandle(self.indexes.len() - 1))
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
    fn seahash() {
        // Ensure we can separate fields to create a distinct hash.
        let mut one = seahash::SeaHasher::default();
        one.write("norknoog".as_bytes());
        one.write_u8(0);
        one.write("donk".as_bytes());

        let mut two = seahash::SeaHasher::default();
        two.write("nork".as_bytes());
        one.write_u8(0);
        two.write("noogdonk".as_bytes());

        assert_ne!(one.finish(), two.finish());
    }

    #[test]
    fn finds_row() {
        let file = File::new(
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
            file.find_table_row(condition, None)
        );
    }

    #[test]
    fn finds_row_with_index() {
        let mut file = File::new(
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(vec!["field1"]).unwrap();

        let condition = btreemap! {
            "field1" => "zirp"
        };

        assert_eq!(
            Some(btreemap! {
                "field1" => "zirp",
                "field2" => "zurp",
            }),
            file.find_table_row(condition, Some(handle))
        );
    }

    #[test]
    fn doesnt_find_row() {
        let file = File::new(
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let condition = btreemap! {
            "field1" => "zorp"
        };

        assert_eq!(None, file.find_table_row(condition, None));
    }

    #[test]
    fn doesnt_find_row_with_index() {
        let mut file = File::new(
            vec![
                vec!["zip".to_string(), "zup".to_string()],
                vec!["zirp".to_string(), "zurp".to_string()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(vec!["field1"]).unwrap();

        let condition = btreemap! {
            "field1" => "zorp"
        };

        assert_eq!(None, file.find_table_row(condition, Some(handle)));
    }
}
