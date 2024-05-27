//! Handles enrichment tables for `type = mmdb`.
//! Enrichment data is loaded from any database in [MaxMind][maxmind] format.
//!
//! [maxmind]: https://maxmind.com
use std::{fs, net::IpAddr, sync::Arc, time::SystemTime};

use maxminddb::{MaxMindDBError, Reader};
use vector_lib::configurable::configurable_component;
use vector_lib::enrichment::{Case, Condition, IndexHandle, Table};
use vrl::value::{ObjectMap, Value};

use crate::config::{EnrichmentTableConfig, GenerateConfig};

/// Configuration for the `mmdb` enrichment table.
#[derive(Clone, Debug, Eq, PartialEq)]
#[configurable_component(enrichment_table("mmdb"))]
pub struct MmdbConfig {
    /// Path to the [MaxMind][maxmind] database
    ///
    /// [maxmind]: https://maxmind.com
    pub path: String,
}

impl GenerateConfig for MmdbConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: "/path/to/GeoLite2-City.mmdb".to_string(),
        })
        .unwrap()
    }
}

impl EnrichmentTableConfig for MmdbConfig {
    async fn build(
        &self,
        _: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Mmdb::new(self.clone())?))
    }
}

#[derive(Clone)]
/// A struct that implements [vector_lib::enrichment::Table] to handle loading enrichment data from a MaxMind database.
pub struct Mmdb {
    config: MmdbConfig,
    dbreader: Arc<maxminddb::Reader<Vec<u8>>>,
    last_modified: SystemTime,
}

impl Mmdb {
    /// Creates a new Mmdb struct from the provided config.
    pub fn new(config: MmdbConfig) -> crate::Result<Self> {
        let dbreader = Arc::new(Reader::open_readfile(config.path.clone())?);

        // Check if we can read database with dummy Ip.
        let ip = IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED);
        let result = dbreader.lookup::<ObjectMap>(ip).map(|_| ());

        match result {
            Ok(_) | Err(MaxMindDBError::AddressNotFoundError(_)) => Ok(Mmdb {
                last_modified: fs::metadata(&config.path)?.modified()?,
                dbreader,
                config,
            }),
            Err(error) => Err(error.into()),
        }
    }

    fn lookup(&self, ip: IpAddr, select: Option<&[String]>) -> Option<ObjectMap> {
        let data = self.dbreader.lookup::<ObjectMap>(ip).ok()?;

        if let Some(fields) = select {
            let mut filtered = Value::from(ObjectMap::new());
            let mut data_value = Value::from(data);
            for field in fields {
                filtered.insert(
                    field.as_str(),
                    data_value
                        .remove(field.as_str(), false)
                        .unwrap_or(Value::Null),
                );
            }
            filtered.into_object()
        } else {
            Some(data)
        }
    }
}

impl Table for Mmdb {
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
        index: Option<IndexHandle>,
    ) -> Result<ObjectMap, String> {
        let mut rows = self.find_table_rows(case, condition, select, index)?;

        match rows.pop() {
            Some(row) if rows.is_empty() => Ok(row),
            Some(_) => Err("More than 1 row found".to_string()),
            None => Err("IP not found".to_string()),
        }
    }

    /// Search the enrichment table data with the given condition.
    /// All conditions must match (AND).
    /// Can return multiple matched records
    fn find_table_rows<'a>(
        &self,
        _: Case,
        condition: &'a [Condition<'a>],
        select: Option<&[String]>,
        _: Option<IndexHandle>,
    ) -> Result<Vec<ObjectMap>, String> {
        match condition.first() {
            Some(_) if condition.len() > 1 => Err("Only one condition is allowed".to_string()),
            Some(Condition::Equals { value, .. }) => {
                let ip = value
                    .to_string_lossy()
                    .parse::<IpAddr>()
                    .map_err(|_| "Invalid IP address".to_string())?;
                Ok(self
                    .lookup(ip, select)
                    .map(|values| vec![values])
                    .unwrap_or_default())
            }
            Some(_) => Err("Only equality condition is allowed".to_string()),
            None => Err("IP condition must be specified".to_string()),
        }
    }

    /// Hints to the enrichment table what data is going to be searched to allow it to index the
    /// data in advance.
    ///
    /// # Errors
    /// Errors if the fields are not in the table.
    fn add_index(&mut self, _: Case, fields: &[&str]) -> Result<IndexHandle, String> {
        match fields.len() {
            0 => Err("IP field is required".to_string()),
            1 => Ok(IndexHandle(0)),
            _ => Err("Only one field is allowed".to_string()),
        }
    }

    /// Returns a list of the field names that are in each index
    fn index_fields(&self) -> Vec<(Case, Vec<String>)> {
        Vec::new()
    }

    /// Returns true if the underlying data has changed and the table needs reloading.
    fn needs_reload(&self) -> bool {
        matches!(fs::metadata(&self.config.path)
            .and_then(|metadata| metadata.modified()),
            Ok(modified) if modified > self.last_modified)
    }
}

impl std::fmt::Debug for Mmdb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Maxmind database {})", self.config.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrl::value::Value;

    #[test]
    fn city_partial_lookup() {
        let values = find_select(
            "2.125.160.216",
            "tests/data/GeoIP2-City-Test.mmdb",
            Some(&[
                "location.latitude".to_string(),
                "location.longitude".to_string(),
            ]),
        )
        .unwrap();

        let mut expected = ObjectMap::new();
        expected.insert(
            "location".into(),
            ObjectMap::from([
                ("latitude".into(), Value::from(51.75)),
                ("longitude".into(), Value::from(-1.25)),
            ])
            .into(),
        );

        assert_eq!(values, expected);
    }

    #[test]
    fn isp_lookup() {
        let values = find("208.192.1.2", "tests/data/GeoIP2-ISP-Test.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("autonomous_system_number".into(), 701i64.into());
        expected.insert(
            "autonomous_system_organization".into(),
            "MCI Communications Services, Inc. d/b/a Verizon Business".into(),
        );
        expected.insert("isp".into(), "Verizon Business".into());
        expected.insert("organization".into(), "Verizon Business".into());

        assert_eq!(values, expected);
    }

    #[test]
    fn connection_type_lookup_success() {
        let values = find(
            "201.243.200.1",
            "tests/data/GeoIP2-Connection-Type-Test.mmdb",
        )
        .unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("connection_type".into(), "Corporate".into());

        assert_eq!(values, expected);
    }

    #[test]
    fn lookup_missing() {
        let values = find("10.1.12.1", "tests/data/custom-type.mmdb");

        assert!(values.is_none());
    }

    #[test]
    fn custom_mmdb_type() {
        let values = find("208.192.1.2", "tests/data/custom-type.mmdb").unwrap();

        let mut expected = ObjectMap::new();
        expected.insert("hostname".into(), "custom".into());
        expected.insert(
            "nested".into(),
            ObjectMap::from([
                ("hostname".into(), "custom".into()),
                ("original_cidr".into(), "208.192.1.2/24".into()),
            ])
            .into(),
        );

        assert_eq!(values, expected);
    }

    fn find(ip: &str, database: &str) -> Option<ObjectMap> {
        find_select(ip, database, None)
    }

    fn find_select(ip: &str, database: &str, select: Option<&[String]>) -> Option<ObjectMap> {
        Mmdb::new(MmdbConfig {
            path: database.to_string(),
        })
        .unwrap()
        .find_table_rows(
            Case::Insensitive,
            &[Condition::Equals {
                field: "ip",
                value: ip.into(),
            }],
            select,
            None,
        )
        .unwrap()
        .pop()
    }
}
