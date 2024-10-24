//! Handles enrichment tables for `type = pgtable`.
use std::{collections::HashMap, hash::Hasher};
use std::{path::PathBuf};
use bytes::Bytes;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
use itertools::intersperse;

// use bytes::Bytes;
// use tracing::trace;
use vector_lib::configurable::configurable_component;
// use vector_lib::{conversion::Conversion, TimeZone};
// use vrl::value::{ObjectMap, Value};
use chrono::{DateTime, Utc};
use crate::config::EnrichmentTableConfig;
use snafu::{Snafu};
use postgres_openssl::MakeTlsConnector;
use tokio_postgres::{Config, Error as PgError, NoTls, Row, Statement, Client};
use tokio_postgres::types::Type;
use openssl::{
    error::ErrorStack,
    ssl::{SslConnector, SslMethod},
};
use vrl::value::{ObjectMap, Value};
use futures::executor::block_on;

#[derive(Debug, Snafu)]
enum ConnectError {
    #[snafu(display("failed to create tls connector: {}", source))]
    TlsFailed { source: ErrorStack },
    #[snafu(display("failed to connect ({}): {}", endpoint, source))]
    ConnectionFailed { source: PgError, endpoint: String },
}

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("invalid endpoint: {}", source))]
    InvalidEndpoint { source: PgError },
    // #[snafu(display("host missing"))]
    // HostMissing,
    // #[snafu(display("multiple hosts not supported: {:?}", hosts))]
    // MultipleHostsNotSupported { hosts: Vec<Host> },
}

/// Configuration of TLS when connecting to PostgreSQL.
#[configurable_component]
#[derive(Clone, Debug)]
#[serde(deny_unknown_fields)]
struct PgtableTlsConfig {
    /// Absolute path to an additional CA certificate file.
    ///
    /// The certificate must be in the DER or PEM (X.509) format.
    #[configurable(metadata(docs::examples = "certs/ca.pem"))]
    ca_file: PathBuf,
}

/// Postgres-specific settings
#[configurable_component(enrichment_table("pgtable"))]
#[derive(Clone, Debug, Default)]
pub struct PgtableConfig {
    #[configurable(derived)]
    /// The postgres connection string
    endpoint: String,
    /// Table to query
    table: String,
    /// Columns to project from table
    columns: Vec<String>,
    /// Reload the table when this query returns a newer timestamp
    /// than the last one that was seen. The result set must contain
    /// zero rows or one row, and it must contain a single timestamptz
    /// column. The name of the column does not matter.
    ///
    /// Example: SELECT max(w.updated) FROM widgets w;
    reload_when: Option<String>,
    /// TLS client certificate
    #[configurable(derived)]
    tls: Option<PgtableTlsConfig>,
}
impl_generate_config_from_default!(PgtableConfig);

fn contains_bad_character(name: &String) -> bool {
    if name.chars().any(char::is_control) {
        return true;
    } else if name.contains('"') {
        return true;
    }
    return false;
}

fn surround_with_quotes(name: String) -> String {
  return ["\"", &name, "\""].concat();
}

fn row_to_vec_value(row: Row) -> Vec<Value> {
    let row_len = row.len();
    let mut dst: Vec<Value> = Vec::new();
    for i in 0..row_len {
      let rint : Result<i64, PgError> = row.try_get(i);
      match rint {
        Err(_) => {
          let rstr : Result<String, PgError> = row.try_get(i);
          match rstr {
            Ok(s) => {
              dst.push(Value::Bytes(Bytes::from(s)));
            }
            Err(_) => {
              dst.push(Value::Null);
            }
          }
        }
        Ok(i) => {
          dst.push(Value::Integer(i));
        }
      }
    };
    return dst;
}

impl EnrichmentTableConfig for PgtableConfig {
    async fn build(
        &self,
        _globals: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        // Table names and column names are unconditionally surrounded by
        // double quotes in the `select ... from ...` query, so we prohibit
        // the double quote character from appearing in either of these.
        // We additionally prohibit control characters since their occurrence
        // almost certainly indicates an accident.
        if contains_bad_character(&self.table) {
          return Err("Table name contains prohibited characters".to_string().into());
        }
        if self.columns.iter().any(contains_bad_character) {
          return Err("One or more columns contain prohibited characters".to_string().into());
        }
        // x // Check reload_when with a simple heuristic before even attempting
        // x // to connect to the database. This does not catch every possible
        // x // mistake, but it makes it more likely that the user will get a
        // x // helpful error message instead of a confusing message from postgres.
        // x match &self.reload_when {
        // x     Some(query) => {
        // x         if query.contains("$1") {
        // x         } else {
        // x             return Err("Prepared statement for reload_when must use parameter $1".to_string().into());
        // x         }
        // x     }
        // x     None => {}
        // x }
        let config: Config = self.endpoint.parse()?;
        let client = match &self.tls {
            Some(tls_config) => {
                let mut builder = SslConnector::builder(SslMethod::tls_client())?;
                builder.set_ca_file(tls_config.ca_file.clone())?;
                let connector = MakeTlsConnector::new(builder.build());

                let (client, connection) = config.connect(connector).await?;
                tokio::spawn(connection);
                client
            }
            None => {
                let (client, connection) = config.connect(NoTls).await?;
                tokio::spawn(connection);
                client
            }
        };
        let statement = match &self.reload_when {
            Some(query) => { Some(client.prepare_typed(query,&[Type::TIMESTAMPTZ]).await?) }
            None => { None }
        };
        let updated = match &statement {
            Some(s) => match client.query_opt(s,&[]).await? {
                Some(row) => { row.try_get(0)? }
                None => { DateTime::UNIX_EPOCH }
            }
            None => { DateTime::UNIX_EPOCH }
        };
        let columns_with_commas : Vec<String> = intersperse(self.columns.clone().into_iter().map(surround_with_quotes), ",".to_string()).collect();
        let select_query = format!("SELECT {} FROM \"{}\"", &columns_with_commas.concat(), &self.table);
        let rows = client.query(&select_query, &[]).await?;
        let data = rows.into_iter().map(row_to_vec_value).collect();
        Ok(Box::new(Pgtable::new(data, client, updated, statement, self.columns.clone())))
    }
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

/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a postgres database. This only happens once at load time.
pub struct Pgtable {
    data: Vec<Vec<Value>>,
    client: Client,
    updated: DateTime<Utc>,
    statement: Option<Statement>,
    headers: Vec<String>,
    indexes: Vec<(
        Case,
        Vec<usize>,
        HashMap<u64, Vec<usize>, hash_hasher::HashBuildHasher>,
    )>,
}

impl Pgtable {
    /// Creates a new [File] based on the provided config.
    pub fn new(
        data: Vec<Vec<Value>>,
        client : Client,
        updated : DateTime<Utc>,
        statement: Option<Statement>,
        headers: Vec<String>,
    ) -> Self {
        Self {
            data,
            client,
            updated,
            statement,
            headers,
            indexes: Vec::new(),
        }
    }
    fn column_index(&self, col: &str) -> Option<usize> {
        self.headers.iter().position(|header| header == col)
    }
    fn add_columns(&self, select: Option<&[String]>, row: &[Value]) -> ObjectMap {
        self.headers
            .iter()
            .zip(row)
            .filter(|(header, _)| {
                select
                    .map(|select| select.contains(header))
                    // If no select is passed, we assume all columns are included
                    .unwrap_or(true)
            })
            .map(|(header, col)| (header.as_str().into(), col.clone()))
            .collect()
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
    /// Sequentially searches through the iterator for the given condition.
    fn sequential<'a, I>(
        &'a self,
        data: I,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
    ) -> impl Iterator<Item = ObjectMap> + 'a
    where
        I: Iterator<Item = &'a Vec<Value>> + 'a,
    {
        data.filter_map(move |row| {
            if self.row_equals(case, condition, row) {
                Some(self.add_columns(select, row))
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

impl Table for Pgtable {
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&'a [String]>,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, String> {
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
    ) -> Result<Vec<ObjectMap>, String> {
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

    /// Checks the modified timestamp of the data file to see if data has changed.
    fn needs_reload(&self) -> bool {
        let old = self.updated;
        let new = match &self.statement {
            Some(s) => match block_on(self.client.query_opt(s,&[])) {
                Ok(res) => match res {
                    Some(row) => match row.try_get(0) {
                        Ok(t) => { t }
                        Err(e) => { return true } // On error, request a reload? Not sure what to do here.
                    }
                    None => { DateTime::UNIX_EPOCH }
                }
                Err(e) => { return true } // On error, request a reload? Not sure what to do here.
            }
            None => { DateTime::UNIX_EPOCH }
        };
        if new > old {
            self.updated = new;
            return true;
        } else {
            return false;
        }
    }
}
