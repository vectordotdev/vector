use std::{collections::BTreeMap, fs, net::IpAddr, sync::Arc, time::SystemTime};

use enrichment::{Case, Condition, IndexHandle, Table};
use maxminddb::{
    geoip2::{City, Isp},
    MaxMindDBError, Reader,
};
use serde::{Deserialize, Serialize};
use value::Value;

use crate::config::{EnrichmentTableConfig, EnrichmentTableDescription, GenerateConfig};

// MaxMind GeoIP database files have a type field we can use to recognize specific
// products. If we encounter one of these two types, we look for ASN/ISP information;
// otherwise we expect to be working with a City database.
const ASN_DATABASE_TYPE: &str = "GeoLite2-ASN";
const ISP_DATABASE_TYPE: &str = "GeoIP2-ISP";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct GeoipConfig {
    pub path: String,
    #[serde(default = "default_locale")]
    pub locale: String,
}

// valid locales are: “de”, "en", “es”, “fr”, “ja”, “pt-BR”, “ru”, and “zh-CN”
//
// https://dev.maxmind.com/geoip/docs/databases/city-and-country?lang=en
//
// TODO try to determine the system locale and use that as default if it matches a valid locale?
fn default_locale() -> String {
    "en".to_string()
}

impl GenerateConfig for GeoipConfig {
    fn generate_config() -> toml::Value {
        toml::Value::try_from(Self {
            path: "/path/to/GeoLite2-City.mmdb".to_string(),
            locale: default_locale(),
        })
        .unwrap()
    }
}

inventory::submit! {
    EnrichmentTableDescription::new::<GeoipConfig>("geoip")
}

#[async_trait::async_trait]
#[typetag::serde(name = "geoip")]
impl EnrichmentTableConfig for GeoipConfig {
    async fn build(
        &self,
        _: &crate::config::GlobalOptions,
    ) -> crate::Result<Box<dyn Table + Send + Sync>> {
        Ok(Box::new(Geoip::new(self.clone())?))
    }
}

#[derive(Clone)]
pub struct Geoip {
    config: GeoipConfig,
    dbreader: Arc<maxminddb::Reader<Vec<u8>>>,
    last_modified: SystemTime,
}

impl Geoip {
    pub fn new(config: GeoipConfig) -> crate::Result<Self> {
        let table = Geoip {
            last_modified: fs::metadata(&config.path)?.modified()?,
            dbreader: Arc::new(Reader::open_readfile(config.path.clone())?),
            config,
        };

        // Check if we can read database with dummy Ip.
        let ip = IpAddr::V4(std::net::Ipv4Addr::new(0, 0, 0, 0));
        let result = if table.has_isp_db() {
            table.dbreader.lookup::<Isp>(ip).map(|_| ())
        } else {
            table.dbreader.lookup::<City>(ip).map(|_| ())
        };

        match result {
            Ok(_) | Err(MaxMindDBError::AddressNotFoundError(_)) => (),
            Err(error) => return Err(error.into()),
        }

        Ok(table)
    }

    fn has_isp_db(&self) -> bool {
        self.dbreader.metadata.database_type == ASN_DATABASE_TYPE
            || self.dbreader.metadata.database_type == ISP_DATABASE_TYPE
    }

    fn lookup(&self, ip: IpAddr, select: Option<&[String]>) -> Option<BTreeMap<String, Value>> {
        let mut map = BTreeMap::new();
        let mut add_field = |key: &str, value: Option<Value>| {
            if select
                .map(|fields| fields.iter().any(|field| field == key))
                .unwrap_or(true)
            {
                map.insert(key.to_string(), value.unwrap_or(Value::Null));
            }
        };

        if self.has_isp_db() {
            let data = self.dbreader.lookup::<Isp>(ip).ok()?;

            add_field(
                "autonomous_system_number",
                data.autonomous_system_number.map(Into::into),
            );
            add_field(
                "autonomous_system_organization",
                data.autonomous_system_organization.map(Into::into),
            );

            add_field("isp", data.isp.map(Into::into));

            add_field("organization", data.organization.map(Into::into));
        } else {
            let data = self.dbreader.lookup::<City>(ip).ok()?;

            add_field(
                "city_name",
                data.city
                    .as_ref()
                    .and_then(|c| c.names.as_ref())
                    .and_then(|names| names.get(&*self.config.locale))
                    .map(|&name| name.into()),
            );

            add_field(
                "continent_code",
                data.continent.and_then(|c| c.code).map(Into::into),
            );

            let country = data.country.as_ref();
            add_field(
                "country_code",
                country.and_then(|country| country.iso_code).map(Into::into),
            );
            add_field(
                "country_name",
                country
                    .and_then(|country| {
                        country
                            .names
                            .as_ref()
                            .and_then(|names| names.get(&*self.config.locale))
                    })
                    .map(|&name| name.into()),
            );

            let location = data.location.as_ref();
            add_field(
                "timezone",
                location
                    .and_then(|location| location.time_zone)
                    .map(Into::into),
            );
            add_field(
                "latitude",
                location
                    .and_then(|location| location.latitude)
                    .map(Into::into),
            );
            add_field(
                "longitude",
                location
                    .and_then(|location| location.longitude)
                    .map(Into::into),
            );
            add_field(
                "metro_code",
                location
                    .and_then(|location| location.metro_code)
                    .map(Into::into),
            );

            // last subdivision is most specific per https://github.com/maxmind/GeoIP2-java/blob/39385c6ce645374039450f57208b886cf87ade47/src/main/java/com/maxmind/geoip2/model/AbstractCityResponse.java#L96-L107
            let subdivision = data.subdivisions.as_ref().and_then(|s| s.last());
            add_field(
                "region_name",
                subdivision
                    .and_then(|subdivision| {
                        subdivision
                            .names
                            .as_ref()
                            .and_then(|names| names.get(&*self.config.locale))
                    })
                    .map(|&name| name.into()),
            );
            add_field(
                "region_code",
                subdivision
                    .and_then(|subdivision| subdivision.iso_code)
                    .map(Into::into),
            );

            add_field(
                "postal_code",
                data.postal.and_then(|p| p.code).map(Into::into),
            );
        }

        Some(map)
    }
}

impl Table for Geoip {
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
    ) -> Result<BTreeMap<String, Value>, String> {
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
    ) -> Result<Vec<BTreeMap<String, Value>>, String> {
        match condition.get(0) {
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

impl std::fmt::Debug for Geoip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Geoip {} database {})",
            self.config.locale, self.config.path
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn city_lookup() {
        let values = find("2.125.160.216", "tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut expected = BTreeMap::<String, Value>::new();
        expected.insert("city_name".to_string(), "Boxford".into());
        expected.insert("country_code".to_string(), "GB".into());
        expected.insert("continent_code".to_string(), "EU".into());
        expected.insert("country_name".to_string(), "United Kingdom".into());
        expected.insert("region_code".to_string(), "WBK".into());
        expected.insert("region_name".to_string(), "West Berkshire".into());
        expected.insert("timezone".to_string(), "Europe/London".into());
        expected.insert("latitude".to_string(), Value::from(51.75));
        expected.insert("longitude".to_string(), Value::from(-1.25));
        expected.insert("postal_code".to_string(), "OX1".into());
        expected.insert("metro_code".to_string(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn city_partial_lookup() {
        let values = find_select(
            "2.125.160.216",
            "tests/data/GeoIP2-City-Test.mmdb",
            Some(&["latitude".to_string(), "longitude".to_string()]),
        )
        .unwrap();

        let mut expected = BTreeMap::<String, Value>::new();
        expected.insert("latitude".to_string(), Value::from(51.75));
        expected.insert("longitude".to_string(), Value::from(-1.25));

        assert_eq!(values, expected);
    }

    #[test]
    fn city_lookup_partial_results() {
        let values = find("67.43.156.9", "tests/data/GeoIP2-City-Test.mmdb").unwrap();

        let mut expected = BTreeMap::<String, Value>::new();
        expected.insert("city_name".to_string(), Value::Null);
        expected.insert("country_code".to_string(), "BT".into());
        expected.insert("country_name".to_string(), "Bhutan".into());
        expected.insert("continent_code".to_string(), "AS".into());
        expected.insert("region_code".to_string(), Value::Null);
        expected.insert("region_name".to_string(), Value::Null);
        expected.insert("timezone".to_string(), "Asia/Thimphu".into());
        expected.insert("latitude".to_string(), Value::from(27.5));
        expected.insert("longitude".to_string(), Value::from(90.5));
        expected.insert("postal_code".to_string(), Value::Null);
        expected.insert("metro_code".to_string(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn city_lookup_no_results() {
        let values = find("10.1.12.1", "tests/data/GeoIP2-City-Test.mmdb");

        assert!(values.is_none());
    }

    #[test]
    fn isp_lookup() {
        let values = find("208.192.1.2", "tests/data/GeoIP2-ISP-Test.mmdb").unwrap();

        let mut expected = BTreeMap::<String, Value>::new();
        expected.insert("autonomous_system_number".to_string(), 701i64.into());
        expected.insert(
            "autonomous_system_organization".to_string(),
            "MCI Communications Services, Inc. d/b/a Verizon Business".into(),
        );
        expected.insert("isp".to_string(), "Verizon Business".into());
        expected.insert("organization".to_string(), "Verizon Business".into());

        assert_eq!(values, expected);
    }

    #[test]
    fn isp_lookup_partial_results() {
        let values = find("2600:7000::1", "tests/data/GeoLite2-ASN-Test.mmdb").unwrap();

        let mut expected = BTreeMap::<String, Value>::new();
        expected.insert("autonomous_system_number".to_string(), 6939i64.into());
        expected.insert(
            "autonomous_system_organization".to_string(),
            "Hurricane Electric, Inc.".into(),
        );
        expected.insert("isp".to_string(), Value::Null);
        expected.insert("organization".to_string(), Value::Null);

        assert_eq!(values, expected);
    }

    #[test]
    fn isp_lookup_no_results() {
        let values = find("10.1.12.1", "tests/data/GeoLite2-ASN-Test.mmdb");

        assert!(values.is_none());
    }

    fn find(ip: &str, database: &str) -> Option<BTreeMap<String, Value>> {
        find_select(ip, database, None)
    }

    fn find_select(
        ip: &str,
        database: &str,
        select: Option<&[String]>,
    ) -> Option<BTreeMap<String, Value>> {
        Geoip::new(GeoipConfig {
            path: database.to_string(),
            locale: default_locale(),
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
