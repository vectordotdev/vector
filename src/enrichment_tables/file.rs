use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};
use bytes::Bytes;
use enrichment::{Condition, IndexHandle, Table};
use serde::{Deserialize, Serialize};
use shared::{conversion::Conversion, datetime::TimeZone};
use std::collections::{BTreeMap, HashMap};
use std::hash::Hasher;
use std::path::PathBuf;
use tracing::trace;
use vrl::Value;

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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum SchemaType {
    String,
    Date,
    DateTime,
    Integer,
    Float,
    Boolean,
}

#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
struct FileConfig {
    file: FileC,
    #[serde(default)]
    schema: HashMap<String, SchemaType>,
}

const fn default_delimiter() -> char {
    ','
}

impl FileConfig {
    fn parse_column(
        &self,
        timezone: TimeZone,
        column: &str,
        row: usize,
        value: &str,
    ) -> Result<Value, String> {
        use chrono::TimeZone;

        Ok(match self.schema.get(column) {
            Some(SchemaType::Date) => Value::Timestamp(
                chrono::FixedOffset::east(0)
                    .from_utc_datetime(
                        &chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                            .map_err(|_| {
                                format!("unable to parse date {} found in row {}", value, row)
                            })?
                            .and_hms(0, 0, 0),
                    )
                    .into(),
            ),
            Some(SchemaType::DateTime) => Conversion::Timestamp(timezone)
                .convert(Bytes::copy_from_slice(value.as_bytes()))
                .map_err(|_| format!("unable to parse datetime {} found in row {}", value, row))?,
            Some(SchemaType::Integer) => Conversion::Integer
                .convert(Bytes::copy_from_slice(value.as_bytes()))
                .map_err(|_| format!("unable to parse integer {} found in row {}", value, row))?,
            Some(SchemaType::Float) => Conversion::Boolean
                .convert(Bytes::copy_from_slice(value.as_bytes()))
                .map_err(|_| format!("unable to parse integer {} found in row {}", value, row))?,
            Some(SchemaType::Boolean) => Conversion::Boolean
                .convert(Bytes::copy_from_slice(value.as_bytes()))
                .map_err(|_| format!("unable to parse integer {} found in row {}", value, row))?,
            Some(SchemaType::String) | None => value.into(),
        })
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        let Encoding::Csv {
            include_headers,
            delimiter,
        } = self.file.encoding;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(include_headers)
            .delimiter(delimiter as u8)
            .from_path(&self.file.path)?;

        let headers = if include_headers {
            reader
                .headers()?
                .iter()
                .map(|col| col.to_string())
                .collect::<Vec<_>>()
        } else {
            // If there are no headers in the datafile we make headers as the numerical index of
            // the column.
            match reader.records().next() {
                Some(Ok(row)) => (0..row.len()).map(|idx| idx.to_string()).collect(),
                _ => Vec::new(),
            }
        };

        let data = reader
            .records()
            .map(|row| {
                Ok(row?
                    .iter()
                    .enumerate()
                    .map(|(idx, col)| self.parse_column(globals.timezone, &headers[idx], idx, col))
                    .collect::<Result<Vec<_>, String>>()?)
            })
            .collect::<crate::Result<Vec<_>>>()?;

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
    data: Vec<Vec<Value>>,
    headers: Vec<String>,
    indexes: Vec<(
        Vec<usize>,
        HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>,
    )>,
}

impl File {
    pub fn new(data: Vec<Vec<Value>>, headers: Vec<String>) -> Self {
        Self {
            data,
            headers,
            indexes: Vec::new(),
        }
    }

    fn column_index(&self, col: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == col)
    }

    /// Currently all matches are case insensitive.
    /// TODO We want to add a configuration option to allow for case sensitive searches. This will
    /// allow for more performant matches.
    fn row_equals(&self, condition: &[Condition], row: &[Value]) -> bool {
        condition.iter().all(|condition| match condition {
            Condition::Equals { field, value } => match self.column_index(field) {
                None => false,
                Some(idx) => match (&row[idx], value) {
                    (Value::Bytes(bytes1), Value::Bytes(bytes2)) => {
                        match (std::str::from_utf8(bytes1), std::str::from_utf8(bytes2)) {
                            (Ok(s1), Ok(s2)) => s1.to_lowercase() == s2.to_lowercase(),
                            (Err(_), Err(_)) => bytes1 == bytes2,
                            _ => false,
                        }
                    }
                    (value1, value2) => value1 == value2,
                },
            },
            Condition::BetweenDates { field, from, to } => match self.column_index(field) {
                None => false,
                Some(idx) => match row[idx] {
                    Value::Timestamp(date) => from <= &date && &date <= to,
                    _ => false,
                },
            },
        })
    }

    fn add_columns(&self, row: &[Value]) -> BTreeMap<String, Value> {
        self.headers
            .iter()
            .zip(row)
            .map(|(header, col)| (header.clone(), col.clone()))
            .collect()
    }

    /// Order the fields in the index according to the position they are found in the header
    fn normalize_index_fields(&self, index: &[&str]) -> Vec<usize> {
        // Get the positions of the fields we are indexing
        self.headers
            .iter()
            .enumerate()
            .filter_map(|(idx, col)| {
                if index.contains(&col.as_ref()) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    /// Creates an index with the given fields.
    /// Uses seahash to create a hash of the data that is used as the key in a hashmap lookup to
    /// the index of the row in the data.
    ///
    /// Ensure fields that are searched via a comparison are not included in the index!
    fn index_data(
        &self,
        fieldidx: &[usize],
    ) -> Result<HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>, String> {
        let mut index = HashMap::with_capacity_and_hasher(
            self.data.len(),
            hash_hasher::HashBuildHasher::default(),
        );

        for (idx, row) in self.data.iter().enumerate() {
            let mut hash = seahash::SeaHasher::default();
            for idx in fieldidx {
                hash_value(&mut hash, &row[*idx])?;
            }

            let key = hash.finish();

            let entry = index.entry(key).or_insert_with(Vec::new);
            entry.push(idx);
        }

        index.shrink_to_fit();

        Ok(index)
    }

    /// Sequentially searches through the iterator for the given condition.
    fn sequential<'a, I>(
        &'a self,
        data: I,
        condition: &'a [Condition<'a>],
    ) -> impl Iterator<Item = BTreeMap<String, Value>> + 'a
    where
        I: Iterator<Item = &'a Vec<Value>> + 'a,
    {
        data.filter_map(move |row| {
            if self.row_equals(condition, &*row) {
                Some(self.add_columns(&*row))
            } else {
                None
            }
        })
    }

    fn indexed<'a>(
        &'a self,
        condition: &'a [Condition<'a>],
        handle: IndexHandle,
    ) -> Result<Option<&'a Vec<usize>>, String> {
        // The index to use has been passed, we can use this to search the data.
        // We are assuming that the caller has passed an index that represents the fields
        // being passed in the condition.
        let mut hash = seahash::SeaHasher::default();

        for header in self.headers.iter() {
            if let Some(Condition::Equals { value, .. }) = condition.iter().find(
                |condition| matches!(condition, Condition::Equals { field, .. } if field == header),
            ) {
                hash_value(&mut hash, value)?;
            }
        }

        let key = hash.finish();

        let IndexHandle(handle) = handle;
        Ok(self.indexes[handle].1.get(&key))
    }
}

/// Adds the bytes from the given value to the hash.
/// Each field is terminated by a `0` value to separate the fields
fn hash_value(hasher: &mut seahash::SeaHasher, value: &Value) -> Result<(), String> {
    match value {
        Value::Bytes(bytes) => {
            hasher.write(
                std::str::from_utf8(bytes)
                    .map_err(|_| "column contains invalid utf".to_string())?
                    .to_lowercase()
                    .as_bytes(),
            );
        }
        value => {
            let bytes: bytes::Bytes = value.encode_as_bytes()?;
            hasher.write(&bytes);
        }
    }

    hasher.write_u8(0);

    Ok(())
}

/// Returns an error if the iterator doesn't yield exactly one result.
fn single_or_err<I, T>(mut iter: T) -> Result<I, String>
where
    T: Iterator<Item = I>,
{
    let result = iter.next();

    if iter.next().is_some() {
        // More than one row has been found.
        Err("more than one row found".to_string())
    } else {
        result.ok_or_else(|| "no rows found".to_string())
    }
}

impl Table for File {
    fn find_table_row<'a>(
        &self,
        condition: &'a [Condition<'a>],
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        match index {
            None => {
                // No index has been passed so we need to do a Sequential Scan.
                single_or_err(self.sequential(self.data.iter(), condition))
            }
            Some(handle) => {
                let result = self
                    .indexed(condition, handle)?
                    .ok_or_else(|| "no rows found in index".to_string())?
                    .iter()
                    .map(|idx| &self.data[*idx]);

                // Perform a sequential scan over the indexed result.
                single_or_err(self.sequential(result, condition))
            }
        }
    }

    fn find_table_rows<'a>(
        &self,
        condition: &'a [Condition<'a>],
        index: Option<IndexHandle>,
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        match index {
            None => {
                // No index has been passed so we need to do a Sequential Scan.
                Ok(self.sequential(self.data.iter(), condition).collect())
            }
            Some(handle) => {
                // Perform a sequential scan over the indexed result.
                Ok(self
                    .sequential(
                        self.indexed(condition, handle)?
                            .iter()
                            .flat_map(|results| results.iter().map(|idx| &self.data[*idx])),
                        condition,
                    )
                    .collect())
            }
        }
    }

    fn add_index(&mut self, fields: &[&str]) -> Result<IndexHandle, String> {
        let normalized = self.normalize_index_fields(fields);
        match self.indexes.iter().position(|index| index.0 == normalized) {
            Some(pos) => {
                // This index already exists
                Ok(IndexHandle(pos))
            }
            None => {
                let index = self.index_data(&normalized)?;
                self.indexes.push((normalized, index));

                // The returned index handle is the position of the index in our list of indexes.
                Ok(IndexHandle(self.indexes.len() - 1))
            }
        }
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
    use chrono::TimeZone;
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
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zirp"),
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
    fn duplicate_indexes() {
        let mut file = File::new(
            Vec::new(),
            vec![
                "field1".to_string(),
                "field2".to_string(),
                "field3".to_string(),
            ],
        );

        let handle1 = file.add_index(&["field2", "field3"]);
        let handle2 = file.add_index(&["field3", "field2"]);

        assert_eq!(handle1, handle2);
        assert_eq!(1, file.indexes.len());
    }

    #[test]
    fn finds_row_with_index() {
        let mut file = File::new(
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(&["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zirp"),
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
    fn finds_rows_with_index() {
        let mut file = File::new(
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
                vec!["zip".into(), "zoop".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(&["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zip"),
        };

        assert_eq!(
            Ok(vec![
                btreemap! {
                    "field1" => "zip",
                    "field2" => "zup",
                },
                btreemap! {
                    "field1" => "zip",
                    "field2" => "zoop",
                }
            ]),
            file.find_table_rows(&[condition], Some(handle))
        );
    }

    #[test]
    fn finds_row_with_dates() {
        let mut file = File::new(
            vec![
                vec![
                    "zip".into(),
                    Value::Timestamp(chrono::Utc.ymd(2015, 12, 7).and_hms(0, 0, 0)),
                ],
                vec![
                    "zip".into(),
                    Value::Timestamp(chrono::Utc.ymd(2016, 12, 7).and_hms(0, 0, 0)),
                ],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(&["field1"]).unwrap();

        let conditions = [
            Condition::Equals {
                field: "field1",
                value: "zip".into(),
            },
            Condition::BetweenDates {
                field: "field2",
                from: chrono::Utc.ymd(2016, 1, 1).and_hms(0, 0, 0),
                to: chrono::Utc.ymd(2017, 1, 1).and_hms(0, 0, 0),
            },
        ];

        assert_eq!(
            Ok(btreemap! {
                "field1" => "zip",
                "field2" => Value::Timestamp(chrono::Utc.ymd(2016, 12, 7).and_hms(0, 0, 0)),
            }),
            file.find_table_row(&conditions, Some(handle))
        );
    }

    #[test]
    fn doesnt_find_row() {
        let file = File::new(
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zorp"),
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
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(&["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zorp"),
        };

        assert_eq!(
            Err("no rows found in index".to_string()),
            file.find_table_row(&[condition], Some(handle))
        );
    }
}
