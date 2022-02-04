use std::{
    collections::{BTreeMap, HashMap},
    fs,
    hash::Hasher,
    path::PathBuf,
    time::SystemTime,
};

use bytes::Bytes;
use enrichment::{Case, Condition, IndexHandle, Table};
use serde::{Deserialize, Serialize};
use tracing::trace;
use vector_common::{conversion::Conversion, datetime::TimeZone};
use vrl::Value;

use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription};

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
pub struct FileConfig {
    file: FileC,
    #[serde(default)]
    schema: HashMap<String, String>,
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
            Some(format) => {
                let mut split = format.splitn(2, '|').map(|segment| segment.trim());

                match (split.next(), split.next()) {
                    (Some("date"), None) => Value::Timestamp(
                        chrono::FixedOffset::east(0)
                            .from_utc_datetime(
                                &chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                                    .map_err(|_| {
                                        format!(
                                            "unable to parse date {} found in row {}",
                                            value, row
                                        )
                                    })?
                                    .and_hms(0, 0, 0),
                            )
                            .into(),
                    ),
                    (Some("date"), Some(format)) => Value::Timestamp(
                        chrono::FixedOffset::east(0)
                            .from_utc_datetime(
                                &chrono::NaiveDate::parse_from_str(value, format)
                                    .map_err(|_| {
                                        format!(
                                            "unable to parse date {} found in row {}",
                                            value, row
                                        )
                                    })?
                                    .and_hms(0, 0, 0),
                            )
                            .into(),
                    ),
                    _ => {
                        let conversion =
                            Conversion::parse(format, timezone).map_err(|err| err.to_string())?;
                        conversion
                            .convert(Bytes::copy_from_slice(value.as_bytes()))
                            .map_err(|_| {
                                format!("unable to parse {} found in row {}", value, row)
                            })?
                    }
                }
            }
            None => value.into(),
        })
    }

    fn load_file(
        &self,
        timezone: TimeZone,
    ) -> crate::Result<(Vec<String>, Vec<Vec<Value>>, SystemTime)> {
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
                    .map(|(idx, col)| self.parse_column(timezone, &headers[idx], idx, col))
                    .collect::<Result<Vec<_>, String>>()?)
            })
            .collect::<crate::Result<Vec<_>>>()?;

        trace!(
            "Loaded enrichment file {} with headers {:?}.",
            self.file.path.to_str().unwrap_or("path with invalid utf"),
            headers
        );

        let modified = fs::metadata(&self.file.path)?.modified()?;

        Ok((headers, data, modified))
    }
}

#[async_trait::async_trait]
#[typetag::serde(name = "file")]
impl EnrichmentTableConfig for FileConfig {
    async fn build(
        &self,
        globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        let (headers, data, modified) = self.load_file(globals.timezone)?;

        Ok(Box::new(File::new(self.clone(), modified, data, headers)))
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<FileConfig>("file")
}

impl_generate_config_from_default!(FileConfig);

#[derive(Clone)]
pub struct File {
    config: FileConfig,
    last_modified: SystemTime,
    data: Vec<Vec<Value>>,
    headers: Vec<String>,
    indexes: Vec<(
        Case,
        Vec<usize>,
        HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>,
    )>,
}

impl File {
    pub fn new(
        config: FileConfig,
        last_modified: SystemTime,
        data: Vec<Vec<Value>>,
        headers: Vec<String>,
    ) -> Self {
        Self {
            config,
            last_modified,
            data,
            headers,
            indexes: Vec::new(),
        }
    }

    fn column_index(&self, col: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == col)
    }

    /// Does the given row match all the conditions specified?
    fn row_equals(&self, case: Case, condition: &[Condition], row: &[Value]) -> bool {
        condition.iter().all(|condition| match condition {
            Condition::Equals { field, value } => match self.column_index(field) {
                None => false,
                Some(idx) => match (case, &row[idx], value) {
                    (Case::Insensitive, Value::Bytes(bytes1), Value::Bytes(bytes2)) => {
                        match (std::str::from_utf8(bytes1), std::str::from_utf8(bytes2)) {
                            (Ok(s1), Ok(s2)) => s1.to_lowercase() == s2.to_lowercase(),
                            (Err(_), Err(_)) => bytes1 == bytes2,
                            _ => false,
                        }
                    }
                    (_, value1, value2) => value1 == value2,
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

    fn add_columns(&self, select: Option<&[String]>, row: &[Value]) -> BTreeMap<String, Value> {
        self.headers
            .iter()
            .zip(row)
            .filter(|(header, _)| {
                select
                    .map(|select| select.contains(header))
                    // If no select is passed, we assume all columns are included
                    .unwrap_or(true)
            })
            .map(|(header, col)| (header.clone(), col.clone()))
            .collect()
    }

    /// Order the fields in the index according to the position they are found in the header.
    fn normalize_index_fields(&self, index: &[&str]) -> Result<Vec<usize>, String> {
        // Get the positions of the fields we are indexing
        let normalized = self
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

        if normalized.len() != index.len() {
            let missing = index
                .iter()
                .filter_map(|col| {
                    if self.headers.iter().any(|header| header == *col) {
                        None
                    } else {
                        Some(col.to_string())
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(format!("field(s) '{}' missing from dataset", missing))
        } else {
            Ok(normalized)
        }
    }

    /// Creates an index with the given fields.
    /// Uses seahash to create a hash of the data that is used as the key in a hashmap lookup to
    /// the index of the row in the data.
    ///
    /// Ensure fields that are searched via a comparison are not included in the index!
    fn index_data(
        &self,
        fieldidx: &[usize],
        case: Case,
    ) -> Result<HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>, String> {
        let mut index = HashMap::with_capacity_and_hasher(
            self.data.len(),
            hash_hasher::HashBuildHasher::default(),
        );

        for (idx, row) in self.data.iter().enumerate() {
            let mut hash = seahash::SeaHasher::default();

            for idx in fieldidx {
                hash_value(&mut hash, case, &row[*idx])?;
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
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
    ) -> impl Iterator<Item = BTreeMap<String, Value>> + 'a
    where
        I: Iterator<Item = &'a Vec<Value>> + 'a,
    {
        data.filter_map(move |row| {
            if self.row_equals(case, condition, &*row) {
                Some(self.add_columns(select, &*row))
            } else {
                None
            }
        })
    }

    fn indexed<'a>(
        &'a self,
        case: Case,
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
                hash_value(&mut hash, case, value)?;
            }
        }

        let key = hash.finish();

        let IndexHandle(handle) = handle;
        Ok(self.indexes[handle].2.get(&key))
    }
}

/// Adds the bytes from the given value to the hash.
/// Each field is terminated by a `0` value to separate the fields
fn hash_value(hasher: &mut seahash::SeaHasher, case: Case, value: &Value) -> Result<(), String> {
    match value {
        Value::Bytes(bytes) => match case {
            Case::Sensitive => hasher.write(bytes),
            Case::Insensitive => hasher.write(
                std::str::from_utf8(bytes)
                    .map_err(|_| "column contains invalid utf".to_string())?
                    .to_lowercase()
                    .as_bytes(),
            ),
        },
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
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        index: Option<IndexHandle>,
    ) -> Result<BTreeMap<String, Value>, String> {
        match index {
            None => {
                // No index has been passed so we need to do a Sequential Scan.
                single_or_err(self.sequential(self.data.iter(), case, condition, select))
            }
            Some(handle) => {
                let result = self
                    .indexed(case, condition, handle)?
                    .ok_or_else(|| "no rows found in index".to_string())?
                    .iter()
                    .map(|idx| &self.data[*idx]);

                // Perform a sequential scan over the indexed result.
                single_or_err(self.sequential(result, case, condition, select))
            }
        }
    }

    fn find_table_rows<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        index: Option<IndexHandle>,
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        match index {
            None => {
                // No index has been passed so we need to do a Sequential Scan.
                Ok(self
                    .sequential(self.data.iter(), case, condition, select)
                    .collect())
            }
            Some(handle) => {
                // Perform a sequential scan over the indexed result.
                Ok(self
                    .sequential(
                        self.indexed(case, condition, handle)?
                            .iter()
                            .flat_map(|results| results.iter().map(|idx| &self.data[*idx])),
                        case,
                        condition,
                        select,
                    )
                    .collect())
            }
        }
    }

    fn add_index(&mut self, case: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        let normalized = self.normalize_index_fields(fields)?;
        match self
            .indexes
            .iter()
            .position(|index| index.0 == case && index.1 == normalized)
        {
            Some(pos) => {
                // This index already exists
                Ok(IndexHandle(pos))
            }
            None => {
                let index = self.index_data(&normalized, case)?;
                self.indexes.push((case, normalized, index));
                // The returned index handle is the position of the index in our list of indexes.
                Ok(IndexHandle(self.indexes.len() - 1))
            }
        }
    }

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        self.indexes
            .iter()
            .map(|index| {
                let (case, fields, _) = index;
                (
                    *case,
                    fields
                        .iter()
                        .map(|idx| self.headers[*idx].clone())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>()
    }

    /// Checks the modified timestamp of the data file to see if data has changed.
    fn needs_reload(&self) -> bool {
        matches!(fs::metadata(&self.config.file.path)
            .and_then(|metadata| metadata.modified()),
            Ok(modified) if modified > self.last_modified)
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
    use chrono::TimeZone;
    use vector_common::btreemap;

    use super::*;

    #[test]
    fn parse_column() {
        let mut schema = HashMap::new();
        schema.insert("col1".to_string(), " string ".to_string());
        schema.insert("col2".to_string(), " date ".to_string());
        schema.insert("col3".to_string(), "date|%m/%d/%Y".to_string());
        schema.insert("col3-spaces".to_string(), "date | %m %d %Y".to_string());
        schema.insert("col4".to_string(), "timestamp|%+".to_string());
        schema.insert("col4-spaces".to_string(), "timestamp | %+".to_string());
        schema.insert("col5".to_string(), "int".to_string());
        let config = FileConfig {
            file: Default::default(),
            schema,
        };

        assert_eq!(
            Ok(Value::from("zork")),
            config.parse_column(Default::default(), "col1", 1, "zork")
        );

        assert_eq!(
            Ok(Value::from(chrono::Utc.ymd(2020, 3, 5).and_hms(0, 0, 0))),
            config.parse_column(Default::default(), "col2", 1, "2020-03-05")
        );

        assert_eq!(
            Ok(Value::from(chrono::Utc.ymd(2020, 3, 5).and_hms(0, 0, 0))),
            config.parse_column(Default::default(), "col3", 1, "03/05/2020")
        );

        assert_eq!(
            Ok(Value::from(chrono::Utc.ymd(2020, 3, 5).and_hms(0, 0, 0))),
            config.parse_column(Default::default(), "col3-spaces", 1, "03 05 2020")
        );

        assert_eq!(
            Ok(Value::from(
                chrono::Utc.ymd(2001, 7, 7).and_hms_micro(15, 4, 0, 26490)
            )),
            config.parse_column(
                Default::default(),
                "col4",
                1,
                "2001-07-08T00:34:00.026490+09:30"
            )
        );

        assert_eq!(
            Ok(Value::from(
                chrono::Utc.ymd(2001, 7, 7).and_hms_micro(15, 4, 0, 26490)
            )),
            config.parse_column(
                Default::default(),
                "col4-spaces",
                1,
                "2001-07-08T00:34:00.026490+09:30"
            )
        );

        assert_eq!(
            Ok(Value::from(42)),
            config.parse_column(Default::default(), "col5", 1, "42")
        );
    }

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
            Default::default(),
            SystemTime::now(),
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
            file.find_table_row(Case::Sensitive, &[condition], None, None)
        );
    }

    #[test]
    fn duplicate_indexes() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            Vec::new(),
            vec![
                "field1".to_string(),
                "field2".to_string(),
                "field3".to_string(),
            ],
        );

        let handle1 = file.add_index(Case::Sensitive, &["field2", "field3"]);
        let handle2 = file.add_index(Case::Sensitive, &["field3", "field2"]);

        assert_eq!(handle1, handle2);
        assert_eq!(1, file.indexes.len());
    }

    #[test]
    fn errors_on_missing_columns() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            Vec::new(),
            vec![
                "field1".to_string(),
                "field2".to_string(),
                "field3".to_string(),
            ],
        );

        let error = file.add_index(Case::Sensitive, &["apples", "field2", "bananas"]);
        assert_eq!(
            Err("field(s) 'apples, bananas' missing from dataset".to_string()),
            error
        )
    }

    #[test]
    fn finds_row_with_index() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(Case::Sensitive, &["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zirp"),
        };

        assert_eq!(
            Ok(btreemap! {
                "field1" => "zirp",
                "field2" => "zurp",
            }),
            file.find_table_row(Case::Sensitive, &[condition], None, Some(handle))
        );
    }

    #[test]
    fn finds_rows_with_index_case_sensitive() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
                vec!["zip".into(), "zoop".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(Case::Sensitive, &["field1"]).unwrap();

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
            file.find_table_rows(
                Case::Sensitive,
                &[Condition::Equals {
                    field: "field1",
                    value: Value::from("zip"),
                }],
                None,
                Some(handle)
            )
        );

        assert_eq!(
            Ok(vec![]),
            file.find_table_rows(
                Case::Sensitive,
                &[Condition::Equals {
                    field: "field1",
                    value: Value::from("ZiP"),
                }],
                None,
                Some(handle)
            )
        );
    }

    #[test]
    fn selects_columns() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            vec![
                vec!["zip".into(), "zup".into(), "zoop".into()],
                vec!["zirp".into(), "zurp".into(), "zork".into()],
                vec!["zip".into(), "zoop".into(), "zibble".into()],
            ],
            vec![
                "field1".to_string(),
                "field2".to_string(),
                "field3".to_string(),
            ],
        );

        let handle = file.add_index(Case::Sensitive, &["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zip"),
        };

        assert_eq!(
            Ok(vec![
                btreemap! {
                    "field1" => "zip",
                    "field3" => "zoop",
                },
                btreemap! {
                    "field1" => "zip",
                    "field3" => "zibble",
                }
            ]),
            file.find_table_rows(
                Case::Sensitive,
                &[condition],
                Some(&["field1".to_string(), "field3".to_string()]),
                Some(handle)
            )
        );
    }

    #[test]
    fn finds_rows_with_index_case_insensitive() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
                vec!["zip".into(), "zoop".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(Case::Insensitive, &["field1"]).unwrap();

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
            file.find_table_rows(
                Case::Insensitive,
                &[Condition::Equals {
                    field: "field1",
                    value: Value::from("zip"),
                }],
                None,
                Some(handle)
            )
        );

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
            file.find_table_rows(
                Case::Insensitive,
                &[Condition::Equals {
                    field: "field1",
                    value: Value::from("ZiP"),
                }],
                None,
                Some(handle)
            )
        );
    }

    #[test]
    fn finds_row_with_dates() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
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

        let handle = file.add_index(Case::Sensitive, &["field1"]).unwrap();

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
            file.find_table_row(Case::Sensitive, &conditions, None, Some(handle))
        );
    }

    #[test]
    fn doesnt_find_row() {
        let file = File::new(
            Default::default(),
            SystemTime::now(),
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
            file.find_table_row(Case::Sensitive, &[condition], None, None)
        );
    }

    #[test]
    fn doesnt_find_row_with_index() {
        let mut file = File::new(
            Default::default(),
            SystemTime::now(),
            vec![
                vec!["zip".into(), "zup".into()],
                vec!["zirp".into(), "zurp".into()],
            ],
            vec!["field1".to_string(), "field2".to_string()],
        );

        let handle = file.add_index(Case::Sensitive, &["field1"]).unwrap();

        let condition = Condition::Equals {
            field: "field1",
            value: Value::from("zorp"),
        };

        assert_eq!(
            Err("no rows found in index".to_string()),
            file.find_table_row(Case::Sensitive, &[condition], None, Some(handle))
        );
    }
}
