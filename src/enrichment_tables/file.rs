use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};
use enrichment::{Condition, IndexHandle, Table};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;
use std::path::PathBuf;
use tracing::trace;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Encoding {
    Csv {
        #[serde(default = "crate::serde::default_true")]
        include_headers: bool,
        #[serde(default = "default_delimiter")]
        delimiter: char,
    },
}

impl Default for Encoding {
    fn default() -> Self {
        Self::Csv {
            include_headers: true,
            delimiter: default_delimiter(),
        }
    }
}

#[derive(Deserialize, Serialize, Default, Debug, Eq, PartialEq, Clone)]
struct FileC {
    path: PathBuf,
    encoding: Encoding,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
struct FileConfig {
    file: FileC,
}

const fn default_delimiter() -> char {
    ','
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        let Encoding::Csv {
            include_headers,
            delimiter,
        } = self.file.encoding;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(include_headers)
            .delimiter(delimiter as u8)
            .from_path(&self.file.path)?;

        let data = reader
            .records()
            .map(|row| Ok(row?.iter().map(|col| col.to_string()).collect::<Vec<_>>()))
            .collect::<crate::Result<Vec<_>>>()?;

        let headers = if include_headers {
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

        trace!(
            "Loaded enrichment file {} with headers {:?}.",
            self.file.path.to_str().unwrap_or("path with invalid utf"),
            headers
        );

        Ok(Box::new(File::new(data, headers)))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

#[derive(Clone)]
pub struct File {
    data: Vec<Vec<String>>,
    headers: Vec<String>,
    indexes: Vec<HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>>,
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

    fn row_equals(&self, condition: &[Condition], row: &[String]) -> bool {
        condition.iter().all(|condition| match condition {
            Condition::Equals { field, value } => match self.column_index(field) {
                None => false,
                Some(idx) => row[idx].to_lowercase() == value.to_lowercase(),
            },
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
    /// Uses seahash to create a hash of the data that is used as the key in a hashmap lookup to
    /// the index of the row in the data.
    fn index_data(&self, index: &[&str]) -> HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher> {
        // Get the positions of the fields we are indexing
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

        let mut index = HashMap::with_capacity_and_hasher(
            self.data.len(),
            hash_hasher::HashBuildHasher::default(),
        );

        for (idx, row) in self.data.iter().enumerate() {
            let mut hash = seahash::SeaHasher::default();
            for idx in &fieldidx {
                hash.write(row[*idx].to_lowercase().as_bytes());
                hash.write_u8(0);
            }

            let key = hash.finish();

            let entry = index.entry(key).or_insert_with(Vec::new);
            entry.push(idx);
        }

        index.shrink_to_fit();

        index
    }
}

impl Table for File {
    fn find_table_row<'a>(
        &self,
        condition: &'a [Condition<'a>],
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, String>, String> {
        match index {
            None => {
                // No index has been passed so we need to do a Sequential Scan.
                let mut found = self.data.iter().filter_map(|row| {
                    if self.row_equals(condition, &*row) {
                        Some(self.add_columns(row))
                    } else {
                        None
                    }
                });

                let result = found.next();

                if found.next().is_some() {
                    // More than one row has been found.
                    Err("more than one row found".to_string())
                } else {
                    result.ok_or_else(|| "no rows found".to_string())
                }
            }
            Some(IndexHandle(handle)) => {
                // The index to use has been passed, we can use this to search the data.
                // We are assuming that the caller has passed an index that represents the fields
                // being passed in the condition.
                let mut hash = seahash::SeaHasher::default();

                for header in self.headers.iter() {
                    if let Some(Condition::Equals { value, .. }) = condition.iter().find(|condition|
                    {
                        matches!(condition, Condition::Equals { field, .. } if field == header)
                    })
                    {
                            hash.write(value.to_lowercase().as_bytes());
                            hash.write_u8(0);
                    }
                }

                let key = hash.finish();

                self.indexes[handle]
                    .get(&key)
                    .ok_or_else(|| "no rows found".to_string())
                    .and_then(|rows| {
                        // Ensure we have exactly one result.
                        if rows.len() == 1 {
                            Ok(self.add_columns(&self.data[rows[0]]))
                        } else if rows.is_empty() {
                            Err("no rows found".to_string())
                        } else {
                            Err(format!("{} rows found", rows.len()))
                        }
                    })
            }
        }
    }

    fn add_index(&mut self, fields: &[&str]) -> Result<IndexHandle, String> {
        self.indexes.push(self.index_data(fields));

        // The returned index handle is the position of the index in our list of indexes.
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
        one.write(b"norknoog");
        one.write_u8(0);
        one.write(b"donk");

        let mut two = seahash::SeaHasher::default();
        two.write(b"nork");
        one.write_u8(0);
        two.write(b"noogdonk");

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

        let condition = Condition::Equals {
            field: "field1",
            value: "zirp".to_string(),
        };

        assert_eq!(
            Ok(btreemap! {
                "field1" => "zirp",
                "field2" => "zurp",
            }),
            file.find_table_row(&[condition], None)
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

        let handle = file.add_index(&["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: "zirp".to_string(),
        };

        assert_eq!(
            Ok(btreemap! {
                "field1" => "zirp",
                "field2" => "zurp",
            }),
            file.find_table_row(&[condition], Some(handle))
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

        let condition = Condition::Equals {
            field: "field1",
            value: "zorp".to_string(),
        };

        assert_eq!(
            Err("no rows found".to_string()),
            file.find_table_row(&[condition], None)
        );
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

        let handle = file.add_index(&["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: "zorp".to_string(),
        };

        assert_eq!(
            Err("no rows found".to_string()),
            file.find_table_row(&[condition], Some(handle))
        );
    }
}
