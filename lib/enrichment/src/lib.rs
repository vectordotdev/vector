#![deny(warnings)]

pub mod find_enrichment_table_records;
pub mod get_enrichment_table_record;
pub mod tables;

#[cfg(test)]
mod test_util;
mod vrl_util;

use dyn_clone::DynClone;
use indoc::indoc;
pub use tables::{TableRegistry, TableSearch};
use vrl::{
    compiler::Function,
    value::{ObjectMap, Value},
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct IndexHandle(pub usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Condition<'a> {
    /// Condition exactly matches the field value.
    Equals { field: &'a str, value: Value },
    /// The date in the field is between from and to (inclusive).
    BetweenDates {
        field: &'a str,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    },
    /// The date in the field is greater than or equal to `from`.
    FromDate {
        field: &'a str,
        from: chrono::DateTime<chrono::Utc>,
    },
    /// The date in the field is less than or equal to `to`.
    ToDate {
        field: &'a str,
        to: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Sensitive,
    Insensitive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NoRowsFound(String),
    MoreThanOneRowFound(String),
    InvalidInput(String),
    TableError(String),
}

impl Error {
    pub fn message(&self) -> String {
        match self {
            Error::NoRowsFound(message) => message.clone(),
            Error::MoreThanOneRowFound(message) => message.clone(),
            Error::InvalidInput(message) => message.clone(),
            Error::TableError(message) => message.clone(),
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for Error {}

impl From<Error> for vrl::prelude::ExpressionError {
    fn from(error: Error) -> Self {
        vrl::prelude::ExpressionError::Error {
            message: error.message(),
            labels: vec![],
            notes: vec![],
        }
    }
}

/// Enrichment tables represent additional data sources that can be used to enrich the event data
/// passing through Vector.
pub trait Table: DynClone {
    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    ///
    /// # Errors
    /// Errors if no rows, or more than 1 row is found.
    fn find_table_row<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&[String]>,
        wildcard: Option<&Value>,
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, Error>;

    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    /// Can return multiple matched records
    fn find_table_rows<'a>(
        &self,
        case: Case,
        condition: &'a [Condition<'a>],
        select: Option<&[String]>,
        wildcard: Option<&Value>,
        index: Option<IndexHandle>,
    ) -> Result<Vec<ObjectMap>, Error>;

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    ///
    /// # Errors
    /// Errors if the fields are not in the table.
    fn add_index(&mut self, case: Case, fields: &[&str]) -> Result<IndexHandle, Error>;

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)>;

    /// Returns true if the underlying data has changed and the table needs reloading.
    fn needs_reload(&self) -> bool;
}

dyn_clone::clone_trait_object!(Table);

pub fn vrl_functions() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(get_enrichment_table_record::GetEnrichmentTableRecord) as _,
        Box::new(find_enrichment_table_records::FindEnrichmentTableRecords) as _,
    ]
}

pub(crate) const ENRICHMENT_TABLE_EXPLAINER: &str = indoc! {r#"
    For `file` enrichment tables, this condition needs to be a VRL object in which
    the key-value pairs indicate a field to search mapped to a value to search in that field.
    This function returns the rows that match the provided condition(s). _All_ fields need to
    match for rows to be returned; if any fields do not match, then no rows are returned.

    There are currently three forms of search criteria:

    1. **Exact match search**. The given field must match the value exactly. Case sensitivity
       can be specified using the `case_sensitive` argument. An exact match search can use an
       index directly into the dataset, which should make this search fairly "cheap" from a
       performance perspective.

    2. **Wildcard match search**. The given fields specified by the exact match search may also
        be matched exactly to the value provided to the `wildcard` parameter.
        A wildcard match search can also use an index directly into the dataset.

    3. **Date range search**. The given field must be greater than or equal to the `from` date
       and/or less than or equal to the `to` date. A date range search involves
       sequentially scanning through the rows that have been located using any exact match
       criteria. This can be an expensive operation if there are many rows returned by any exact
       match criteria. Therefore, use date ranges as the _only_ criteria when the enrichment
       data set is very small.

    For `geoip` and `mmdb` enrichment tables, this condition needs to be a VRL object with a single key-value pair
    whose value needs to be a valid IP address. Example: `{"ip": .ip }`. If a return field is expected
    and without a value, `null` is used. This table can return the following fields:

    * ISP databases:
        * `autonomous_system_number`
        * `autonomous_system_organization`
        * `isp`
        * `organization`

    * City databases:
        * `city_name`
        * `continent_code`
        * `country_code`
        * `country_name`
        * `region_code`
        * `region_name`
        * `metro_code`
        * `latitude`
        * `longitude`
        * `postal_code`
        * `timezone`

    * Connection-Type databases:
        * `connection_type`

    To use this function, you need to update your configuration to
    include an
    [`enrichment_tables`](/docs/reference/configuration/global-options/#enrichment_tables)
    parameter.
"#};
